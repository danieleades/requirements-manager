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

        let mut tree = Tree::default();

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
