use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hrid {
    /// The kind of requirement, e.g. "URS" or "SYS".
    pub kind: String,

    /// The unique identifier for the requirement, an incrementing index.
    pub id: usize,
}

impl Display for Hrid {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}-{:<03}", self.kind, self.id)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid HRID format: {0}")]
    Syntax(String),
    #[error("Invalid ID in HRID '{0}': expected an integer, got {1}")]
    Id(String, String),
}

impl TryFrom<&str> for Hrid {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let Some((kind, id_str)) = value.split_once('-') else {
            return Err(Error::Syntax(value.to_string()));
        };

        let id = id_str
            .parse()
            .map_err(|_| Error::Id(value.to_string(), id_str.to_string()))?;

        Ok(Self {
            kind: kind.to_string(),
            id,
        })
    }
}
