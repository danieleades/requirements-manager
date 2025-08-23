use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, Result};
use non_empty_string::NonEmptyString;
use uuid::Uuid;

use crate::{
    domain::{self, HridTree},
    storage::dto,
    Hrid, Requirement,
};

/// Filesystem-backed requirements store wrapping an [`HridTree`].
///
/// # Responsibilities
/// - `tree` is the in-memory source of truth for requirements and links.
/// - Each requirement may be persisted as Markdown with YAML front matter via
///   the `dto::markdown` types.
/// - Tracks the *actual* on-disk path for each requirement (`paths`), which may
///   differ from the canonical path derived from the HRID. This supports
///   loading legacy or non-canonical layouts without forced renames.
///
/// # Error policy
/// - **Logic errors** (invariants violated by our own code or data structures)
///   **panic**, because they should be unreachable in non-buggy code. Examples:
///   a UUID present in the tree without an HRID; failing to render markdown for
///   an existing UUID; missing parent HRID for a parent UUID already in the
///   tree.
/// - **External/I/O/data issues** return `anyhow::Result`, e.g. file read/write
///   failures, parse errors during load, or invalid filenames. These are not
///   logic errors.
///
/// # Persistence semantics
/// - Writes use a temp file and atomic rename into place (best-effort across
///   platforms).
/// - On load, for each discovered file we remember the path at which it was
///   found.
/// - `save` writes back to the remembered path; if none is known, it uses the
///   canonical path.
pub struct Directory {
    /// Root directory used to compute canonical locations like
    /// `<root>/<HRID>.md`.
    root: PathBuf,
    /// Logical graph and ID mappings.
    tree: HridTree,
    /// On-disk locations keyed by stable UUID.
    paths: HashMap<Uuid, PathBuf>,
}

impl Directory {
    /// Create an empty, in-memory directory wrapper rooted at `root`.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            tree: HridTree::default(),
            paths: HashMap::new(),
        }
    }

    /// Load a repository of requirements from `root`.
    ///
    /// Scans for `*.md` files (recursively), parses each file with
    /// `dto::markdown::Requirement::from_str`, derives the HRID from the
    /// filename, inserts the node into the in-memory tree, remembers the
    /// *found* path, and defers linking until all nodes are present.
    ///
    /// Notes:
    /// - Your DTO currently does not expose public accessors on the parsed
    ///   `Requirement`. Loading is left partially incomplete until you add
    ///   getters (UUID, created, content, parents) or a conversion function
    ///   into domain types. The skeleton is in place.
    /// - We derive the HRID from the filename stem; ensure filenames are
    ///   `HRID.md`.
    pub fn load(root: PathBuf) -> Result<Self> {
        let mut dir = Self::new(root.clone());

        // Collect discovered nodes and deferred links for a two-phase load.
        struct Deferred {
            child_uuid: Uuid,
            parent_uuid: Uuid,
            parent_hrid_str: String,
            parent_fingerprint: domain::Fingerprint,
        }
        let mut deferred_links: Vec<Deferred> = Vec::new();

        // RECURSIVE scan; requires the `walkdir` crate in Cargo.toml.
        // We treat I/O/parse issues as fallible (return Err), not panics.
        for entry in walkdir::WalkDir::new(&root)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.into_path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }

            let text =
                fs::read_to_string(&path).with_context(|| format!("failed to read {:?}", path))?;

            let parsed: dto::markdown::Requirement = text
                .parse()
                .with_context(|| format!("parse failed for {:?}", path))?;

            // Derive HRID from filename stem (e.g., "REQ-123.md" -> "REQ-123").
            let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
                anyhow::anyhow!("invalid file name for HRID: {:?}", path.file_name())
            })?;
            let hrid = Hrid::from_str(stem)
                .with_context(|| format!("failed to parse HRID `{}` from {:?}", stem, path))?;

            // Convert DTO → domain requirement + UUID and collect links.
            let (uuid, requirement, parents) = dir.dto_to_domain(&parsed).with_context(|| {
                format!(
                    "cannot extract (uuid, requirement, parents) from {:?}",
                    path
                )
            })?;

            // Insert into in-memory tree and remember the actual found path.
            dir.tree
                .insert(hrid.clone(), uuid, requirement)
                .context("insert failed")?;
            dir.paths.insert(uuid, path.clone());

            // Defer linking; we will resolve UUIDs after all inserts.
            deferred_links.extend(parents.into_iter().map(|p| Deferred {
                child_uuid: uuid,
                parent_uuid: p.uuid,
                parent_hrid_str: p.hrid,
                parent_fingerprint: p.fingerprint,
            }));
        }

        // Phase 2: apply links now that all nodes exist.
        // Missing parent/child at this stage is not a logic error; the repository
        // could be incomplete. We attempt to link and skip failures.
        for link in deferred_links {
            // Prefer linking by UUIDs when both ends exist.
            let child_exists = dir.tree.get(&link.child_uuid).is_some();
            let parent_exists = dir.tree.get(&link.parent_uuid).is_some();

            if child_exists && parent_exists {
                if let Err(e) = dir.tree.link(link.child_uuid, link.parent_uuid) {
                    // Non-logic failure (e.g., cycle); surface as load error.
                    return Err(anyhow::anyhow!(
                        "link {:?} -> {:?} failed during load: {}",
                        link.child_uuid,
                        link.parent_uuid,
                        e
                    ));
                }
                continue;
            }

            // Fallback: resolve parent by HRID string from front matter.
            if child_exists {
                if let Ok(parent_hrid) = Hrid::from_str(&link.parent_hrid_str) {
                    if let Some((p_uuid, _)) = dir.tree.get_by_hrid(&parent_hrid) {
                        dir.tree.link(link.child_uuid, *p_uuid).with_context(|| {
                            format!("link {:?} -> {} failed", link.child_uuid, parent_hrid)
                        })?;
                    }
                }
            }
        }

        Ok(dir)
    }

    /// Add a new requirement and persist to disk.
    ///
    /// Returns `(uuid, hrid)`. Writes to the *canonical* path.
    pub fn add(&mut self, kind: NonEmptyString, requirement: Requirement) -> Result<(Uuid, Hrid)> {
        let (uuid, hrid_ref) = self.tree.add(kind, requirement);
        let hrid = hrid_ref.clone();
        self.save(uuid)?; // save will resolve the path and render from self
        Ok((uuid, hrid))
    }

    /// Link two requirements by HRID and persist the **child** to reflect
    /// parent changes.
    ///
    /// Panics on internal invariant violations (e.g., child not found after
    /// successful link).
    pub fn link(&mut self, child: &Hrid, parent: &Hrid) -> Result<()> {
        self.tree.link_by_hrid(child, parent)?;
        let (uuid, _requirement) = self
            .tree
            .get_by_hrid(child)
            .expect("logic error: child HRID missing immediately after successful link");
        self.save(*uuid)?;
        Ok(())
    }

    /// Rename/move any tracked files that are not at their canonical HRID
    /// paths.
    ///
    /// This reconciles the on-disk layout with the in-memory HRIDs.
    pub fn update_hrids(&mut self) -> Result<()> {
        // Use paths as the UUID source; `add`, `store`, and `load` populate it.
        let uuids: Vec<Uuid> = self.paths.keys().copied().collect();

        for uuid in uuids {
            let hrid = self
                .tree
                .hrid(&uuid)
                .expect("logic error: UUID present in paths without HRID in tree")
                .clone();

            let canonical = self.canonical_path(&hrid);
            let current = self
                .paths
                .get(&uuid)
                .cloned()
                .unwrap_or_else(|| canonical.clone());

            if normalize(&current) == normalize(&canonical) {
                self.paths.insert(uuid, current);
                continue;
            }

            if let Some(parent) = canonical.parent() {
                fs::create_dir_all(parent).with_context(|| format!("mkdir -p {:?}", parent))?;
            }
            fs::rename(&current, &canonical)
                .with_context(|| format!("rename {:?} -> {:?}", current, canonical))?;
            self.paths.insert(uuid, canonical);
        }
        Ok(())
    }

    /// Canonical path for `hrid`, e.g. `<root>/REQ-123.md`.
    pub fn canonical_path(&self, hrid: &Hrid) -> PathBuf {
        self.root.join(hrid.to_string()).with_extension("md")
    }

    /// Path to write for `(uuid, hrid)`: the tracked on-disk path if known,
    /// else canonical.
    pub fn path_for(&self, uuid: Uuid, hrid: &Hrid) -> PathBuf {
        self.paths
            .get(&uuid)
            .cloned()
            .unwrap_or_else(|| self.canonical_path(hrid))
    }

    /// Insert a parsed requirement (with known IDs) and persist it.
    ///
    /// `path_hint` allows you to seed the non-canonical path tracking (as from
    /// loader). Panics only for internal invariant breaks; I/O and DTO
    /// issues return `Result`.
    pub fn store(
        &mut self,
        uuid: Uuid,
        hrid: Hrid,
        requirement: Requirement,
        path_hint: Option<PathBuf>,
    ) -> Result<()> {
        self.tree
            .insert(hrid.clone(), uuid, requirement)
            .context("insert failed")?;

        let path = path_hint.unwrap_or_else(|| self.canonical_path(&hrid));
        self.paths.insert(uuid, path);

        self.save(uuid)?;
        Ok(())
    }

    /// Persist a requirement to disk using its known or canonical path.
    ///
    /// Does not alter the in-memory requirement content; it only updates the
    /// `paths` entry to the path that was written.
    ///
    /// Panics for logic errors (e.g., missing HRID for UUID, or failure to
    /// render).
    fn save(&mut self, uuid: Uuid) -> Result<()> {
        // Resolve HRID (logic error if absent).
        let hrid = self
            .tree
            .hrid(&uuid)
            .expect("logic error: UUID has no HRID in tree")
            .clone();

        let path = self.path_for(uuid, &hrid);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("mkdir -p {:?}", parent))?;
        }

        // Render markdown via DTO; logic error if missing.
        let contents = self
            .to_markdown(uuid)
            .expect("logic error: cannot render UUID to markdown");

        // Atomic write: temp file in same dir, then rename over target
        let mut tmp = path.clone();
        tmp.set_extension("md.tmp");

        write_all(&tmp, contents.as_bytes()).with_context(|| format!("write {:?}", tmp))?;
        fs::rename(&tmp, &path).with_context(|| format!("rename {:?} -> {:?}", tmp, path))?;

        self.paths.insert(uuid, path);
        Ok(())
    }

    /// Render the given `uuid` to Markdown via the DTO’s `Display`
    /// implementation.
    ///
    /// Front matter is populated with:
    /// - `_version: 1`
    /// - `uuid`
    /// - `parents` (with uuids, fingerprints, and HRID strings)
    /// - `created` (taken from `domain::Requirement`)
    ///
    /// Panics on internal invariant violations (missing UUID/HRID/parent HRID).
    fn to_markdown(&self, uuid: Uuid) -> Option<String> {
        // Logic errors here should panic; we route through `expect` in callers.
        let (_hrid, requirement) = self
            .tree
            .get(&uuid)
            .expect("logic error: UUID not present in tree");

        // Build parents; each parent UUID must be resolvable to an HRID.
        let parents = self.tree.parents(uuid).map(|(parent_uuid, fingerprint)| {
            let parent_hrid = self
                .tree
                .hrid(&parent_uuid)
                .expect("logic error: parent UUID missing HRID in tree");
            let parent = domain::requirement::Parent {
                hrid: parent_hrid.clone(),
                fingerprint: fingerprint.clone(),
            };
            (parent_uuid, parent)
        });

        let dto_ref = dto::markdown::RequirementRef::new(uuid, requirement, parents);
        Some(dto_ref.to_string())
    }

    /// Helper to convert a parsed DTO to `(uuid, requirement, parents)` for
    /// insertion.
    ///
    /// This requires public accessors or a conversion method on your DTO type.
    /// Until those exist, this returns a `TODO` error rather than panicking.
    /// DTO/parse issues are external-data problems and should be fallible.
    fn dto_to_domain(
        &self,
        _dto: &dto::markdown::Requirement,
    ) -> Result<(Uuid, Requirement, Vec<ParsedParent>)> {
        // TODO: Expose getters on dto::markdown::Requirement:
        //   - uuid() -> Uuid
        //   - created() -> DateTime<Utc>
        //   - content() -> &str
        //   - parents() -> &[{ uuid: Uuid, hrid: String, fingerprint: Fingerprint }]
        //
        // Then:
        //   let uuid = dto.uuid();
        //   let req = Requirement::new_with_created(dto.content().to_owned(),
        // dto.created());   let parents =
        // dto.parents().iter().cloned().map(ParsedParent::from).collect();
        //
        Err(anyhow::anyhow!(
            "DTO → domain conversion is incomplete; add DTO accessors and implement me"
        ))
    }

    // --- test-only helpers ---
    #[cfg(test)]
    pub(crate) fn set_path_for_test(&mut self, uuid: Uuid, path: PathBuf) {
        self.paths.insert(uuid, path);
    }

    #[cfg(test)]
    pub(crate) fn path_of(&self, uuid: &Uuid) -> Option<&Path> {
        self.paths.get(uuid).map(|p| p.as_path())
    }
}

/// Parent record extracted from DTO front matter during load.
#[derive(Clone)]
struct ParsedParent {
    uuid: Uuid,
    hrid: String,
    fingerprint: domain::Fingerprint,
}

// ---------- Utilities ----------

fn write_all(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::fs::OpenOptions;
    let mut f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    f.write_all(bytes)
}

/// Normalize a path for comparisons; falls back to the raw path if it does not
/// exist.
fn normalize(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn req(text: &str) -> Requirement {
        Requirement::new(text.into())
    }

    #[test]
    fn add_writes_canonical_and_tracks_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let mut dir = Directory::new(root.clone());

        let (uuid, hrid) = dir.add("REQ".parse().unwrap(), req("hello")).unwrap();
        let canonical = dir.canonical_path(&hrid);

        // File exists and is tracked at canonical path.
        assert!(
            canonical.exists(),
            "expected file to exist: {:?}",
            canonical
        );
        assert_eq!(dir.path_of(&uuid).unwrap(), canonical.as_path());

        // The file should contain front matter markers and some body.
        let contents = fs::read_to_string(&canonical).unwrap();
        assert!(contents.starts_with("---\n"), "missing YAML front matter");
        assert!(
            contents.contains("uuid:"),
            "should include uuid in YAML; got:\n{}",
            contents
        );
        assert!(
            contents.trim_end().ends_with("hello"),
            "body should contain the requirement content"
        );
    }

    #[test]
    fn save_respects_tracked_noncanonical_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let mut dir = Directory::new(root.clone());

        let (uuid, hrid) = dir.add("REQ".parse().unwrap(), req("one")).unwrap();

        // Move file to a non-canonical location to simulate a legacy layout.
        let noncanon = root.join("misc").join("custom-name.md");
        fs::create_dir_all(noncanon.parent().unwrap()).unwrap();
        fs::rename(dir.canonical_path(&hrid), &noncanon).unwrap();
        dir.set_path_for_test(uuid, noncanon.clone());

        // Re-save; it should write back to the tracked non-canonical path.
        dir.save(uuid).unwrap();

        assert!(noncanon.exists(), "non-canonical path should exist");
        assert_eq!(dir.path_of(&uuid).unwrap(), noncanon.as_path());
    }

    #[test]
    fn update_hrids_renames_to_canonical() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let mut dir = Directory::new(root.clone());

        let (uuid, hrid) = dir.add("REQ".parse().unwrap(), req("hello")).unwrap();

        // Simulate file in a non-canonical location
        let noncanon = root.join("stash").join("note.md");
        fs::create_dir_all(noncanon.parent().unwrap()).unwrap();
        fs::rename(dir.canonical_path(&hrid), &noncanon).unwrap();
        dir.set_path_for_test(uuid, noncanon.clone());

        // Reconcile to canonical.
        dir.update_hrids().unwrap();
        let canonical = dir.canonical_path(&hrid);

        assert!(
            canonical.exists(),
            "canonical file should exist after update"
        );
        assert_eq!(dir.path_of(&uuid).unwrap(), canonical.as_path());
        assert!(
            !noncanon.exists(),
            "old non-canonical file should have been moved"
        );
    }

    #[test]
    #[should_panic(expected = "logic error: UUID has no HRID in tree")]
    fn save_panics_on_logic_error_missing_hrid() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let mut dir = Directory::new(root.clone());

        // Random UUID never inserted -> logic error when saving.
        let bogus = Uuid::new_v4();
        // This should panic because there is no HRID for `bogus`.
        let _ = dir.save(bogus);
    }
}
