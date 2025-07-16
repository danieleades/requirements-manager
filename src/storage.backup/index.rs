use std::{collections::HashMap, fs::File, io::{self, BufReader, BufWriter}, path::Path};

use serde::{Deserialize, Serialize};


#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "version")]
enum Versions {
    #[serde(rename = "1")]
    V1(Index)
}

impl From<Versions> for Index {
    fn from(version: Versions) -> Self {
        match version {
            Versions::V1(index) => index,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(from = "Versions")]
pub struct Index {
    /// A map from the requirement type to the latest existing ID for that type.
    /// 
    /// Used for ensuring human-readable IDs are monotonically increasing.
    latest_ids: HashMap<String, usize>,
}

impl Index {
    pub fn from_file(path: &Path) -> Result<Self, FromFileError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        Ok(serde_yaml::from_reader(reader)?)
    }

    pub fn to_file(&self, path: &Path) -> io::Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_yaml::to_writer(writer, self).map_err(|error| io::Error::other(error))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("failed to import index from file: {0}")]
pub enum FromFileError {
    Io(#[from] io::Error),
    Yaml(#[from] serde_yaml::Error),
}