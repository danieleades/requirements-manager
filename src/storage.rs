use std::path::PathBuf;

use crate::{Requirement, domain::Index};

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

        let mut index = Index::load(&index_path).unwrap_or_default();

        let idx = index.bump_index(kind.to_string());

        let requirement = Requirement::new(format!("{kind}-{idx}"), String::new());

        requirement.save(&self.root).unwrap();

        index.save(&index_path).unwrap();
    }
}
