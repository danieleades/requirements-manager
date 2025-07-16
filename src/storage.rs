use std::path::PathBuf;

use crate::{
    Requirement,
    domain::{Index, requirement::LoadError},
};

/// A filesystem backed store of requirements.
pub struct Directory {
    /// The root of the directory requirements are stored in.
    root: PathBuf,
}

impl Directory {
    pub const fn open(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn add_requirement(&self, kind: &str) {
        let index_path = self.root.join(".index.toml");

        let mut index = match Index::load(&index_path) {
            Ok(index) => index,
            Err(e) => {
                println!("e: {e}");
                Index::default()
            }
        };

        let idx = index.bump_index(kind.to_string());

        dbg!(idx);

        let requirement = Requirement::new(format!("{kind}-{idx}"), String::new());

        requirement.save(&self.root).unwrap();

        index.save(&index_path).unwrap();
    }

    pub fn link_requirement(&self, child: String, parent: String) {
        let mut child = self.load_requirement(child).unwrap().unwrap();
        let parent = self.load_requirement(parent).unwrap().unwrap();

        child.add_parent(parent.uuid());

        child.save(&self.root).unwrap();
    }

    fn load_requirement(&self, hrid: String) -> Option<Result<Requirement, LoadError>> {
        dbg!(&self.root);
        match Requirement::load(&self.root, hrid) {
            Ok(requirement) => Some(Ok(requirement)),
            Err(LoadError::NotFound) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
