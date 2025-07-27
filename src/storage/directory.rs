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
use walkdir::WalkDir;

pub use crate::storage::Tree;
use crate::{
    domain::{
        requirement::{LoadError, Parent},
        Config, Hrid,
    },
    EmptyStringError, Requirement,
};

#[derive(Debug, PartialEq)]
pub struct Loaded(Tree);

#[derive(Debug, PartialEq, Eq)]
pub struct Unloaded;

/// A filesystem backed store of requirements.
pub struct Directory<S> {
    /// The root of the directory requirements are stored in.
    root: PathBuf,
    state: S,
}

impl<S> Directory<S> {
    /// Link two requirements together with a parent-child relationship.
    ///
    /// # Errors
    ///
    /// This method can fail if:
    ///
    /// - either the child or parent requirement file cannot be found
    /// - either the child or parent requirement file cannot be parsed
    /// - the child requirement file cannot be written to
    pub fn link_requirement(&self, child: Hrid, parent: Hrid) -> Result<Requirement, LoadError> {
        let mut child = self.load_requirement(child)?;
        let parent = self.load_requirement(parent)?;

        child.add_parent(
            parent.uuid(),
            Parent {
                hrid: parent.hrid().clone(),
                fingerprint: parent.fingerprint(),
            },
        );

        child.save(&self.root)?;

        Ok(child)
    }

    fn load_requirement(&self, hrid: Hrid) -> Result<Requirement, LoadError> {
        Requirement::load(&self.root, hrid)
    }
}

impl Directory<Unloaded> {
    /// Opens a directory at the given path.
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self {
            root,
            state: Unloaded,
        }
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
    pub fn load_all(self) -> Result<Directory<Loaded>, DirectoryLoadError> {
        let config = load_config(&self.root);
        let md_paths = collect_markdown_paths(&self.root);

        let (requirements, unrecognised_paths): (Vec<_>, Vec<_>) = md_paths
            .par_iter()
            .map(|path| try_load_requirement(path))
            .partition(Result::is_ok);

        let requirements: Vec<_> = requirements.into_iter().map(Result::unwrap).collect();
        let unrecognised_paths: Vec<_> = unrecognised_paths
            .into_iter()
            .map(Result::unwrap_err)
            .collect();

        if !config.allow_unrecognised && !unrecognised_paths.is_empty() {
            return Err(DirectoryLoadError::UnrecognisedFiles(unrecognised_paths));
        }

        let mut tree = Tree::with_capacity(requirements.len());
        for req in requirements {
            tree.insert(req);
        }

        Ok(Directory {
            root: self.root,
            state: Loaded(tree),
        })
    }
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

fn try_load_requirement(path: &Path) -> Result<Requirement, PathBuf> {
    let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().to_string()) else {
        tracing::debug!("Skipping file without valid stem: {}", path.display());
        return Err(path.to_path_buf());
    };

    let Ok(hrid) = Hrid::from_str(&stem) else {
        tracing::debug!("Skipping file with invalid HRID: {}", stem);
        return Err(path.to_path_buf());
    };

    let dir = path.parent().unwrap_or(path).to_path_buf();

    match Requirement::load(&dir, hrid) {
        Ok(req) => Ok(req),
        Err(e) => {
            tracing::debug!(
                "Failed to load requirement from {}: {:?}",
                path.display(),
                e
            );
            Err(path.to_path_buf())
        }
    }
}

impl Directory<Loaded> {
    /// Add a new requirement to the directory.
    ///
    /// # Errors
    ///
    /// This method can fail if:
    ///
    /// - the provided `kind` is an empty string
    /// - the requirement file cannot be written to
    pub fn add_requirement(&mut self, kind: String) -> Result<Requirement, AddRequirementError> {
        let tree = &mut self.state.0;

        let id = tree.next_index(&kind);

        let requirement = Requirement::new(Hrid::new(kind, id)?, String::new());

        requirement.save(&self.root)?;
        tree.insert(requirement.clone());

        tracing::info!("Added requirement: {}", requirement.hrid());

        Ok(requirement)
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
        let tree = &mut self.state.0;
        let updated: Vec<_> = tree.update_hrids().collect();

        let failures = updated
            .iter()
            .filter_map(|&id| {
                let requirement = tree.requirement(id)?;
                requirement.save(&self.root).err().map(|e| {
                    (
                        // TODO: manually constructing the path here is brittle. This logic should
                        // be centralised.
                        self.root
                            .join(requirement.hrid().to_string())
                            .with_extension("md"),
                        e,
                    )
                })
            })
            .collect();

        NonEmpty::from_vec(failures).map_or(Ok(()), |failures| Err(UpdateHridsError { failures }))
    }
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

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::Requirement;

    fn setup_temp_directory() -> (TempDir, Directory<Loaded>) {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let path = tmp.path().to_path_buf();
        (tmp, Directory::new(path).load_all().unwrap())
    }

    #[test]
    fn can_add_requirement() {
        let (_tmp, mut dir) = setup_temp_directory();
        let r1 = dir.add_requirement("REQ".to_string()).unwrap();

        assert_eq!(r1.hrid().to_string(), "REQ-001");

        let loaded =
            Requirement::load(&dir.root, r1.hrid().clone()).expect("should load saved requirement");
        assert_eq!(loaded.uuid(), r1.uuid());
    }

    #[test]
    fn can_add_multiple_requirements_with_incrementing_id() {
        let (_tmp, mut dir) = setup_temp_directory();
        let r1 = dir.add_requirement("REQ".to_string()).unwrap();
        let r2 = dir.add_requirement("REQ".to_string()).unwrap();

        assert_eq!(r1.hrid().to_string(), "REQ-001");
        assert_eq!(r2.hrid().to_string(), "REQ-002");
    }

    #[test]
    fn can_link_two_requirements() {
        let (_tmp, mut dir) = setup_temp_directory();
        let parent = dir.add_requirement("SYS".to_string()).unwrap();
        let child = dir.add_requirement("USR".to_string()).unwrap();

        Directory::new(dir.root.clone())
            .link_requirement(child.hrid().clone(), parent.hrid().clone())
            .unwrap();

        let updated =
            Requirement::load(&dir.root, child.hrid().clone()).expect("should load child");

        let parents: Vec<_> = updated.parents().collect();
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].0, parent.uuid());
        assert_eq!(&parents[0].1.hrid, parent.hrid());
    }

    #[test]
    fn update_hrids_corrects_outdated_parent_hrids() {
        let (_tmp, mut dir) = setup_temp_directory();
        let parent = dir.add_requirement("P".to_string()).unwrap();
        let mut child = dir.add_requirement("C".to_string()).unwrap();

        // Manually corrupt HRID in child's parent info
        child.add_parent(
            parent.uuid(),
            Parent {
                hrid: Hrid::try_from("WRONG-999").unwrap(),
                fingerprint: parent.fingerprint(),
            },
        );
        child.save(&dir.root).unwrap();

        let mut loaded_dir = Directory::new(dir.root.clone()).load_all().unwrap();
        loaded_dir.update_hrids().unwrap();

        let updated = Requirement::load(&loaded_dir.root, child.hrid().clone())
            .expect("should load updated child");
        let (_, parent_ref) = updated.parents().next().unwrap();

        assert_eq!(&parent_ref.hrid, parent.hrid());
    }

    #[test]
    fn load_all_reads_all_saved_requirements() {
        let (_tmp, mut dir) = setup_temp_directory();
        let r1 = dir.add_requirement("X".to_string()).unwrap();
        let r2 = dir.add_requirement("X".to_string()).unwrap();

        let loaded = Directory::new(dir.root.clone()).load_all().unwrap();

        let mut found = 0;
        for i in 1..=2 {
            let hrid = Hrid::from_str(&format!("X-00{i}")).unwrap();
            let req = Requirement::load(&loaded.root, hrid).unwrap();
            if req.uuid() == r1.uuid() || req.uuid() == r2.uuid() {
                found += 1;
            }
        }

        assert_eq!(found, 2);
    }
}
