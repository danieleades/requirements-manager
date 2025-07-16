//! The index tracks important metadata about the collection of requirements

use std::{collections::HashMap, io, path::Path};

use crate::domain::index::storage::{LoadError, TomlIndex};

#[derive(Debug, Default, Clone)]
pub struct Index {
    /// A map from the requirement type to the latest existing ID for that type.
    ///
    /// Used for ensuring human-readable IDs are monotonically increasing.
    kinds: HashMap<String, Kind>,
}

impl Index {
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        Ok(TomlIndex::load(path)?.into())
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        TomlIndex::from(self.clone()).save(path)
    }

    pub fn bump_index(&mut self, kind: String) -> usize {
        let info = self.kinds.entry(kind).or_default();
        info.latest_id += 1;
        info.latest_id
    }
}

#[derive(Debug, Default, Clone)]
pub struct Kind {
    latest_id: usize,
}

mod storage {
    //! This module implements the serialisation of the index to disk using a toml format.
    //!
    //! These types are deliberately duplicated in order to provide loose coupling between the domain type and the on-disk
    //! representation.

    use std::{collections::HashMap, fs, io, path::Path};

    use serde::{Deserialize, Serialize};

    use super::{Index, Kind};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(from = "TomlIndexVersion")]
    #[serde(into = "TomlIndexVersion")]
    pub struct TomlIndex {
        /// A map from the requirement type to the latest existing ID for that type.
        ///
        /// Used for ensuring human-readable IDs are monotonically increasing.
        kinds: HashMap<String, TomlKind>,
    }

    impl TomlIndex {
        pub fn load(path: &Path) -> Result<Self, LoadError> {
            let content = fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        }

        pub fn save(&self, path: &Path) -> io::Result<()> {
            let content = toml::to_string_pretty(self).unwrap();
            fs::write(path, content)
        }
    }

    #[derive(Debug, thiserror::Error)]
    #[error("failed to load from file: {0}")]
    pub enum LoadError {
        Toml(#[from] toml::de::Error),
        Io(#[from] io::Error),
    }

    #[derive(Debug, Default, Clone, Serialize, Deserialize)]
    pub struct TomlKind {
        latest_id: usize,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(tag = "_version")]
    enum TomlIndexVersion {
        #[serde(rename = "1")]
        V1 { kinds: HashMap<String, TomlKind> },
    }

    impl From<TomlIndexVersion> for TomlIndex {
        fn from(version: TomlIndexVersion) -> Self {
            match version {
                TomlIndexVersion::V1 { kinds } => Self { kinds },
            }
        }
    }

    impl From<TomlIndex> for TomlIndexVersion {
        fn from(toml_index: TomlIndex) -> Self {
            let TomlIndex { kinds } = toml_index;
            Self::V1 { kinds }
        }
    }

    impl From<Index> for TomlIndex {
        fn from(index: Index) -> Self {
            Self {
                kinds: index
                    .kinds
                    .into_iter()
                    .map(|(hrid, kind)| (hrid, kind.into()))
                    .collect(),
            }
        }
    }

    impl From<TomlIndex> for Index {
        fn from(toml_index: TomlIndex) -> Self {
            Self {
                kinds: toml_index
                    .kinds
                    .into_iter()
                    .map(|(hrid, kind)| (hrid, kind.into()))
                    .collect(),
            }
        }
    }

    impl From<Kind> for TomlKind {
        fn from(kind: Kind) -> Self {
            let Kind { latest_id } = kind;
            Self { latest_id }
        }
    }

    impl From<TomlKind> for Kind {
        fn from(toml_kind: TomlKind) -> Self {
            let TomlKind { latest_id } = toml_kind;
            Self { latest_id }
        }
    }
}
