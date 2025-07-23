//! A filesystem backed store of requirements
//!
//! The [`Directory`] provides a way to manage requirements stored in a directory structure.
//! It is a wrapper around the filesystem agnostic [`Tree`].

use std::{ffi::OsStr, path::PathBuf};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use walkdir::WalkDir;

use crate::{
    Requirement,
    domain::{
        Config, Hrid,
        requirement::{LoadError, Parent},
    },
};

pub use crate::storage::Tree;

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
    pub fn link_requirement(&self, child: String, parent: String) {
        let mut child = self.load_requirement(child).unwrap().unwrap();
        let parent = self.load_requirement(parent).unwrap().unwrap();

        child.add_parent(
            parent.uuid(),
            Parent {
                hrid: parent.hrid().clone(),
                fingerprint: parent.fingerprint(),
            },
        );

        child.save(&self.root).unwrap();
    }

    fn load_requirement(&self, hrid: String) -> Option<Result<Requirement, LoadError>> {
        match Requirement::load(&self.root, hrid) {
            Ok(requirement) => Some(Ok(requirement)),
            Err(LoadError::NotFound) => None,
            Err(e) => Some(Err(e)),
        }
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
    pub fn load_all(self) -> Directory<Loaded> {
        let paths: Vec<_> = WalkDir::new(&self.root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension() == Some(OsStr::new("md")))
            .map(walkdir::DirEntry::into_path)
            .collect();

        let requirements: Vec<Requirement> = paths
            .par_iter()
            .map(|path| {
                let hrid = path.file_stem().unwrap().to_string_lossy().to_string();
                let directory = path.parent().unwrap().to_path_buf();
                Requirement::load(&directory, hrid).unwrap() // TODO: handle error properly
            })
            .collect();

        let mut tree = Tree::with_capacity(requirements.len());

        for req in requirements {
            tree.insert(req);
        }

        Directory {
            root: self.root,
            state: Loaded(tree),
        }
    }
}

impl Directory<Loaded> {
    /// Add a new requirement to the directory.
    pub fn add_requirement(&mut self, kind: String) -> Requirement {
        let config_path = self.root.join("config.toml");

        let tree = &mut self.state.0;

        let _config = Config::load(&config_path).unwrap_or_else(|e| {
            tracing::debug!("Failed to load config: {e}");
            Config::default()
        });

        let id = tree.next_index(&kind);

        let requirement = Requirement::new(Hrid { kind, id }, String::new());

        requirement.save(&self.root).unwrap();
        tree.insert(requirement.clone());

        tracing::info!("Added requirement: {}", requirement.hrid());

        requirement
    }

    /// Update the human-readable IDs (HRIDs) of all 'parents' references in the requirements.
    ///
    /// These can become out of sync if requirement files are renamed.
    pub fn update_hrids(&mut self) {
        let tree = &mut self.state.0;
        let updated: Vec<_> = tree.update_hrids().collect();

        for id in updated {
            let requirement = tree
                .requirement(id)
                .expect("this just got updated, so we know it exists");
            requirement.save(&self.root).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Requirement;
    use tempfile::TempDir;

    fn setup_temp_directory() -> (TempDir, Directory<Loaded>) {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let path = tmp.path().to_path_buf();
        (tmp, Directory::new(path).load_all())
    }

    #[test]
    fn can_add_requirement() {
        let (_tmp, mut dir) = setup_temp_directory();
        let r1 = dir.add_requirement("REQ".to_string());

        assert_eq!(r1.hrid().to_string(), "REQ-001");

        let loaded = Requirement::load(&dir.root, r1.hrid().to_string())
            .expect("should load saved requirement");
        assert_eq!(loaded.uuid(), r1.uuid());
    }

    #[test]
    fn can_add_multiple_requirements_with_incrementing_id() {
        let (_tmp, mut dir) = setup_temp_directory();
        let r1 = dir.add_requirement("REQ".to_string());
        let r2 = dir.add_requirement("REQ".to_string());

        assert_eq!(r1.hrid().to_string(), "REQ-001");
        assert_eq!(r2.hrid().to_string(), "REQ-002");
    }

    #[test]
    fn can_link_two_requirements() {
        let (_tmp, mut dir) = setup_temp_directory();
        let parent = dir.add_requirement("SYS".to_string());
        let child = dir.add_requirement("USR".to_string());

        Directory::new(dir.root.clone())
            .link_requirement(child.hrid().to_string(), parent.hrid().to_string());

        let updated =
            Requirement::load(&dir.root, child.hrid().to_string()).expect("should load child");

        let parents: Vec<_> = updated.parents().collect();
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].0, parent.uuid());
        assert_eq!(&parents[0].1.hrid, parent.hrid());
    }

    #[test]
    fn update_hrids_corrects_outdated_parent_hrids() {
        let (_tmp, mut dir) = setup_temp_directory();
        let parent = dir.add_requirement("P".to_string());
        let mut child = dir.add_requirement("C".to_string());

        // Manually corrupt HRID in child's parent info
        child.add_parent(
            parent.uuid(),
            Parent {
                hrid: Hrid::try_from("WRONG-999").unwrap(),
                fingerprint: parent.fingerprint(),
            },
        );
        child.save(&dir.root).unwrap();

        let mut loaded_dir = Directory::new(dir.root.clone()).load_all();
        loaded_dir.update_hrids();

        let updated = Requirement::load(&loaded_dir.root, child.hrid().to_string())
            .expect("should load updated child");
        let (_, parent_ref) = updated.parents().next().unwrap();

        assert_eq!(&parent_ref.hrid, parent.hrid());
    }

    #[test]
    fn load_all_reads_all_saved_requirements() {
        let (_tmp, mut dir) = setup_temp_directory();
        let r1 = dir.add_requirement("X".to_string());
        let r2 = dir.add_requirement("X".to_string());

        let loaded = Directory::new(dir.root.clone()).load_all();

        let mut found = 0;
        for i in 1..=2 {
            let hrid = format!("X-00{i}");
            dbg!(&hrid);
            let req = Requirement::load(&loaded.root, hrid).unwrap();
            if req.uuid() == r1.uuid() || req.uuid() == r2.uuid() {
                found += 1;
            }
        }

        assert_eq!(found, 2);
    }
}
