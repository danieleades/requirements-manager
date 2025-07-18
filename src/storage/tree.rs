use std::{collections::HashMap, ffi::OsStr, path::PathBuf};

use uuid::Uuid;
use walkdir::WalkDir;

use crate::Requirement;

/// An in-memory representation of the set of requirements
pub struct Tree {
    /// The requirements in the directory
    ///
    /// represented as a tuple of directory and the [`Requirement`].
    ///
    /// Note that the path doesn't contain the file stem, that's determined by the
    /// requirement's human-readable ID (HRID).
    ///
    /// TODO: address the fact the UUID is duplicated in both the key and the value in this hashmap.
    requirements: Vec<Requirement>,

    /// An index from UUID to a tuple of (requirement index, path to containing directory, HRID)
    index: HashMap<Uuid, (usize, PathBuf, String)>,
}

impl Tree {
    /// Load all requirements at the given path.
    ///
    /// This searches recursively from the root down, and matches all markdown files.
    ///
    /// TODO: only match files that have a name that looks like a HRID
    /// TODO: handle errors
    pub fn load_all(root: PathBuf) -> Self {
        let mut requirements = Vec::new();
        let mut index = HashMap::new();

        let paths = WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension() == Some(OsStr::new("md")))
            .map(walkdir::DirEntry::into_path);

        for (idx, path) in paths.enumerate() {
            let hrid = path.file_stem().unwrap().to_string_lossy().to_string();

            let directory = path.parent().unwrap().to_path_buf();

            let requirement = Requirement::load(&directory, hrid.clone()).unwrap();

            index.insert(requirement.uuid(), (idx, directory, hrid));

            requirements.push(requirement);
        }

        Self {
            requirements,
            index,
        }
    }
    pub fn insert(&mut self, path: PathBuf, requirement: Requirement) {
        // TODO: handle collisions
        self.index.insert(
            requirement.uuid(),
            (
                self.requirements.len(),
                path,
                requirement.hrid().to_string(),
            ),
        );
        self.requirements.push(requirement);
    }

    /// Read all the requirements files and update any incorrect parent HRIDs
    pub fn update_hrids(&mut self) {
        let mut updated = Vec::new();
        for req in &mut self.requirements {
            let id = req.uuid();
            for (parent_id, parent) in req.parents_mut() {
                let actual_hrid = &self.index.get(&parent_id).unwrap().2;
                if &parent.hrid != actual_hrid {
                    updated.push(id);
                    parent.hrid = actual_hrid.to_string();
                }
            }
        }

        for id in updated {
            let (idx, path, _hrid) = self.index.get(&id).unwrap();
            let requirement: Requirement = self.requirements.get(*idx).unwrap().clone();
            requirement.save(path).unwrap();
        }
    }
}
