use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Versions", into = "Versions")]
pub struct Config {
    /// The kinds of requirements that are allowed.
    ///
    /// This is the first component of the HRID.
    /// For example, 'USR' or 'SYS'.
    ///
    /// If this is empty, all kinds are allowed.
    allowed_kinds: Vec<String>,

    /// The number of digits in the HRID.
    ///
    /// Digits are padded to this width with leading zeros.
    ///
    /// This is the second component of the HRID.
    /// For example, '001' (3 digits) or '0001' (4 digits).
    digits: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            allowed_kinds: Vec::new(),
            digits: default_digits(),
        }
    }
}

impl Config {
    /// Loads the configuration from a TOML file at the given path.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file: {e}"))?;
        toml::from_str(&content).map_err(|e| format!("Failed to parse config file: {e}"))
    }
}

const fn default_digits() -> usize {
    3
}

/// The serialized versions of the configuration.
/// This allows for future changes to the configuration format and to the domain type without breaking compatibility.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "_version")]
enum Versions {
    #[serde(rename = "1")]
    V1 {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        allowed_kinds: Vec<String>,

        /// The number of digits in the HRID.
        ///
        /// Digits are padded to this width with leading zeros.
        ///
        /// This is the second component of the HRID.
        /// For example, '001' (3 digits) or '0001' (4 digits).
        #[serde(default = "default_digits")]
        digits: usize,
    },
}

impl From<Versions> for super::Config {
    fn from(versions: Versions) -> Self {
        match versions {
            Versions::V1 {
                allowed_kinds,
                digits,
            } => Self {
                allowed_kinds,
                digits,
            },
        }
    }
}

impl From<super::Config> for Versions {
    fn from(config: super::Config) -> Self {
        Self::V1 {
            allowed_kinds: config.allowed_kinds,
            digits: config.digits,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_returns_default() {
        // Tests that derialising an empty file returns the default configuration.
        let expected = Config::default();
        let actual: Config = toml::from_str(r#"_version = "1""#).unwrap();
        assert_eq!(actual, expected);
    }
}
