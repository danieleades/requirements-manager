//! A filesystem backed store of requirements
//!
//! The [`Directory`] provides a way to manage requirements stored in a
//! directory structure. It is a wrapper around the filesystem agnostic
//! [`Tree`].

use std::{
    ffi::OsStr,
    fmt::{self},
    io,
    path::{Path, PathBuf},
    str::FromStr,
};

use nonempty::NonEmpty;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use uuid::Uuid;
use walkdir::WalkDir;

pub use crate::domain::Tree;
use crate::{
    domain::{
        self,
        requirement::{markdown::MarkdownRequirement, LoadError, Parent},
        Config, Hrid,
    },
    EmptyStringError, Requirement,
};

/// A filesystem backed store of requirements.
pub struct Directory {
    /// The root of the directory requirements are stored in.
    root: PathBuf,
    tree: Tree,
}

// fresh attempt at defining the API
impl Directory {
    /// Get a requirement from the in-memory cache, if present
    #[must_use]
    pub fn requirement(&self, uuid: &Uuid) -> Option<&Requirement> {
        self.tree.requirement(uuid)
    }

    /// Retrieve a requirement from the in-memory cache by HRID, if present
    #[must_use]
    pub fn requirement_by_hrid(&self, hrid: &Hrid) -> Option<&Requirement> {
        self.tree.requirement_by_hrid(hrid)
    }

    /// load a requirement from disk by HRID.
    ///
    /// The canonical path to the requirement is determined by the HRID.
    ///
    /// This method does not update the in-memory cache
    ///
    /// # Errors
    ///
    /// This method can fail if the file isn't at the canonical path, or can't
    /// be parsed
    pub fn load_requirement(&self, hrid: Hrid) -> Result<Requirement, LoadError> {
        let path = self.canonical_path(&hrid);

        let markdown_requirement =
            domain::requirement::markdown::MarkdownRequirement::load(&path, hrid)?;
        Ok(markdown_requirement.into())
    }

    /// Save a requirement to disk
    ///
    /// The canonical path to the requirement is determined by the HRID.
    ///
    /// # Errors
    ///
    /// This method can fail if the canonical path cannot be written to
    pub fn save_requirement(&self, requirement: Requirement) -> io::Result<()> {
        let path = self.canonical_path(requirement.hrid());

        let markdown_requirement = MarkdownRequirement::from(requirement);
        markdown_requirement.save(&path)
    }

    /// Insert a requirement into the in-memory cache.
    ///
    /// If the UUID already exists, this replaces the existing requirement and
    /// the old one is returned.
    ///
    /// This method does not persist the updated requirement to disk
    pub fn insert_requirement(&mut self, requirement: Requirement) -> Option<Requirement> {
        self.tree.insert(requirement)
    }

    /// Insert a requirement into the in-memory cache and persist it to disk.
    ///
    /// If the UUID already exists, this replaces the existing requirement and
    /// the old one is returned.
    ///
    /// # Errors
    ///
    /// This method can fail if the canonical path cannot be written to.
    /// If this occurs, the file will *not* be saved to the in-memory cache.
    pub fn store_requirement(
        &mut self,
        requirement: Requirement,
    ) -> io::Result<Option<Requirement>> {
        self.save_requirement(requirement.clone())?;
        Ok(self.insert_requirement(requirement))
    }

    /// Determine the canonical path for a requirement, based on it's HRID
    fn canonical_path(&self, hrid: &Hrid) -> PathBuf {
        self.root.join(hrid.to_string()).with_extension("md")
    }

    /// Link two requirements together with a parent-child relationship.
    ///
    /// # Errors
    ///
    /// This method can fail if:
    ///
    /// - either the child or parent requirement file cannot be found
    /// - either the child or parent requirement file cannot be parsed
    /// - the child requirement file cannot be written to
    pub fn link_requirement(
        &mut self,
        child: &Hrid,
        parent: Hrid,
    ) -> Result<Requirement, LinkError> {
        let parent_requirement = self
            .tree
            .requirement_by_hrid(&parent)
            .ok_or_else(|| LinkError::NotFound(parent.clone()))?;

        let parent_uuid = parent_requirement.uuid();
        let parent_fingerprint = parent_requirement.fingerprint();

        let mut child = self
            .tree
            .requirement_by_hrid_mut(child)
            .ok_or_else(|| LinkError::NotFound(child.clone()))?
            .clone();

        child.add_parent(
            parent_uuid,
            Parent {
                hrid: parent,
                fingerprint: parent_fingerprint,
            },
        );

        let store_result = self.store_requirement(child.clone())?;
        debug_assert!(
            store_result.is_some(),
            "the requirement must already be in the in-memory cache"
        );

        Ok(child)
    }

    /// Load all requirements from disk
    ///
    /// # Errors
    ///
    /// This method has different behaviour depending on the configuration file
    /// in the requirements root. If `allow_unrecognised` is `true`, then
    /// any files with names that are not valid HRIDs, or any files that cannot
    /// be parsed as requirements, are skipped. if `allow_unrecognised` is
    /// `false` (the default), then any unrecognised or invalid markdown files
    /// in the directory will return an error.
    pub fn load(root: PathBuf) -> Result<Self, DirectoryLoadError> {
        let config = load_config(&root);
        let md_paths = collect_markdown_paths(&root);

        let (requirements, unrecognised_paths): (Vec<_>, Vec<_>) = md_paths
            .par_iter()
            .map(|path| load_requirement_from_canonical_path(path))
            .partition(Result::is_ok);

        let requirements: Vec<_> = requirements.into_iter().map(Result::unwrap).collect();
        let unrecognised_paths: Vec<_> = unrecognised_paths
            .into_iter()
            .map(Result::unwrap_err)
            .map(|LoadFromPathError(e)| e)
            .collect();

        if !config.allow_unrecognised && !unrecognised_paths.is_empty() {
            return Err(DirectoryLoadError::UnrecognisedFiles(unrecognised_paths));
        }

        let mut tree = Tree::with_capacity(requirements.len());
        for req in requirements {
            tree.insert(req);
        }

        Ok(Self { root, tree })
    }

    /// Add a new requirement to both the in-memory cache and the on-disk
    /// storage.
    ///
    /// # Errors
    ///
    /// This can fail if the provided `kind` is an empty string, or if the
    /// canonical filepath cannot be written to.
    #[allow(clippy::missing_panics_doc)]
    pub fn add_requirement(&mut self, kind: String) -> Result<&Requirement, AddRequirementError> {
        let id = self.tree.next_index(&kind);
        let hrid = Hrid::new(kind, id)?;
        let requirement = Requirement::new(hrid, String::new());
        let uuid = requirement.uuid();
        let insert_result = self.store_requirement(requirement)?;
        debug_assert!(
            insert_result.is_none(),
            "new requirement can't already be present!"
        );
        Ok(self.requirement(&uuid).expect("we just inserted this"))
    }

    /// Update the human-readable IDs (HRIDs) of all 'parents' references in the
    /// requirements.
    ///
    /// These can become out of sync if requirement files are renamed.
    ///
    /// # Errors
    ///
    /// This method returns an error if some of the requirements cannot be saved
    /// to disk. This method does *not* fail fast. That is, it will attempt
    /// to save all the requirements before returning the error.
    pub fn update_hrids(&mut self) -> Result<(), UpdateHridsError> {
        let updated: Vec<_> = self.tree.update_hrids().collect();

        let failures = updated
            .iter()
            .filter_map(|&id| {
                let requirement = self.tree.requirement(&id)?.clone();
                let hrid = requirement.hrid().clone();
                self.store_requirement(requirement)
                    .err()
                    .map(|e| (self.canonical_path(&hrid), e))
            })
            .collect();

        NonEmpty::from_vec(failures).map_or(Ok(()), |failures| Err(UpdateHridsError { failures }))
    }
}

/// Load a requirement from the given path, inferring the HRID from the path.
fn load_requirement_from_canonical_path(path: &Path) -> Result<Requirement, LoadFromPathError> {
    let hrid_str = path
        .file_stem()
        .ok_or_else(|| LoadFromPathError(path.to_path_buf()))?;

    let hrid = Hrid::from_str(
        hrid_str
            .to_str()
            .ok_or_else(|| LoadFromPathError(path.to_path_buf()))?,
    )
    .map_err(|_| LoadFromPathError(path.to_path_buf()))?;

    let markdown_requirement =
        MarkdownRequirement::load(path, hrid).map_err(|_| LoadFromPathError(path.to_path_buf()))?;
    Ok(markdown_requirement.into())
}

#[derive(Debug, thiserror::Error)]
#[error("failed to load requirement from path {0}")]
struct LoadFromPathError(PathBuf);

#[derive(Debug, thiserror::Error)]
#[error("Failed to link requirements: {0}")]
pub enum LinkError {
    Io(#[from] io::Error),
    #[error("requirement {0} not found")]
    NotFound(Hrid),
}

#[derive(Debug, thiserror::Error)]
pub enum DirectoryLoadError {
    UnrecognisedFiles(Vec<PathBuf>),
}

impl fmt::Display for DirectoryLoadError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

fn load_config(root: &Path) -> Config {
    let path = root.join("config.toml");
    Config::load(&path).unwrap_or_else(|e| {
        tracing::debug!("Failed to load config: {e}");
        Config::default()
    })
}

fn collect_markdown_paths(root: &PathBuf) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension() == Some(OsStr::new("md")))
        .map(walkdir::DirEntry::into_path)
        .collect()
}

#[derive(Debug, thiserror::Error)]
#[error("failed to add requirement: {0}")]
pub enum AddRequirementError {
    Kind(#[from] EmptyStringError),
    Io(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
pub struct UpdateHridsError {
    failures: NonEmpty<(PathBuf, io::Error)>,
}

impl fmt::Display for UpdateHridsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const MAX_DISPLAY: usize = 5;

        write!(f, "failed to update HRIDS: ")?;

        let total = self.failures.len();

        let displayed_paths: Vec<String> = self
            .failures
            .iter()
            .take(MAX_DISPLAY)
            .map(|(p, _e)| p.display().to_string())
            .collect();

        let msg = displayed_paths.join(", ");

        if total <= MAX_DISPLAY {
            write!(f, "{msg}")
        } else {
            write!(f, "{msg}... (and {} more)", total - MAX_DISPLAY)
        }
    }
}
