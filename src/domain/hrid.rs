use std::fmt::Display;

/// A human-readable identifier (HRID) for a requirement.
///
/// HRIDs are intended to be stable and immutable for the lifetime of a
/// requirement, however separating the HRID from the UUID means that it is
/// possible (but not necessarily recommended) to update them. This can be
/// useful for correcting errors, modifying requirement hierarchies, introducing
/// namespacing, etc.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hrid_creation() {
        let hrid = Hrid {
            kind: "URS".to_string(),
            id: 42,
        };

        assert_eq!(hrid.kind, "URS");
        assert_eq!(hrid.id, 42);
    }

    #[test]
    fn test_hrid_display() {
        let hrid = Hrid {
            kind: "SYS".to_string(),
            id: 1,
        };
        assert_eq!(format!("{hrid}"), "SYS-001");

        let hrid = Hrid {
            kind: "URS".to_string(),
            id: 42,
        };
        assert_eq!(format!("{hrid}"), "URS-042");

        let hrid = Hrid {
            kind: "TEST".to_string(),
            id: 999,
        };
        assert_eq!(format!("{hrid}"), "TEST-999");
    }

    #[test]
    fn test_hrid_display_large_numbers() {
        let hrid = Hrid {
            kind: "BIG".to_string(),
            id: 1000,
        };
        assert_eq!(format!("{hrid}"), "BIG-1000");

        let hrid = Hrid {
            kind: "HUGE".to_string(),
            id: 12345,
        };
        assert_eq!(format!("{hrid}"), "HUGE-12345");
    }

    #[test]
    fn test_try_from_valid() {
        let hrid = Hrid::try_from("URS-001").unwrap();
        assert_eq!(hrid.kind, "URS");
        assert_eq!(hrid.id, 1);

        let hrid = Hrid::try_from("SYS-042").unwrap();
        assert_eq!(hrid.kind, "SYS");
        assert_eq!(hrid.id, 42);

        let hrid = Hrid::try_from("TEST-999").unwrap();
        assert_eq!(hrid.kind, "TEST");
        assert_eq!(hrid.id, 999);
    }

    #[test]
    fn test_try_from_valid_no_leading_zeros() {
        let hrid = Hrid::try_from("URS-1").unwrap();
        assert_eq!(hrid.kind, "URS");
        assert_eq!(hrid.id, 1);

        let hrid = Hrid::try_from("SYS-42").unwrap();
        assert_eq!(hrid.kind, "SYS");
        assert_eq!(hrid.id, 42);
    }

    #[test]
    fn test_try_from_valid_large_numbers() {
        let hrid = Hrid::try_from("BIG-1000").unwrap();
        assert_eq!(hrid.kind, "BIG");
        assert_eq!(hrid.id, 1000);

        let hrid = Hrid::try_from("HUGE-12345").unwrap();
        assert_eq!(hrid.kind, "HUGE");
        assert_eq!(hrid.id, 12345);
    }

    #[test]
    fn test_try_from_invalid_no_dash() {
        let result = Hrid::try_from("URS001");
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Syntax(s) => assert_eq!(s, "URS001"),
            Error::Id(..) => panic!("Expected Syntax error"),
        }
    }

    #[test]
    fn test_try_from_invalid_empty_string() {
        let result = Hrid::try_from("");
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Syntax(s) => assert_eq!(s, ""),
            Error::Id(..) => panic!("Expected Syntax error"),
        }
    }

    #[test]
    fn test_try_from_invalid_only_dash() {
        let result = Hrid::try_from("-");
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Id(hrid, id_part) => {
                assert_eq!(hrid, "-");
                assert_eq!(id_part, "");
            }
            Error::Syntax(_) => panic!("Expected Id error"),
        }
    }

    #[test]
    fn test_try_from_invalid_non_numeric_id() {
        let result = Hrid::try_from("URS-abc");
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Id(hrid, id_part) => {
                assert_eq!(hrid, "URS-abc");
                assert_eq!(id_part, "abc");
            }
            Error::Syntax(_) => panic!("Expected Id error"),
        }
    }

    #[test]
    fn test_try_from_invalid_mixed_id() {
        let result = Hrid::try_from("SYS-12abc");
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Id(hrid, id_part) => {
                assert_eq!(hrid, "SYS-12abc");
                assert_eq!(id_part, "12abc");
            }
            Error::Syntax(_) => panic!("Expected Id error"),
        }
    }

    #[test]
    fn test_try_from_invalid_negative_id() {
        let result = Hrid::try_from("URS--1");
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Id(hrid, id_part) => {
                assert_eq!(hrid, "URS--1");
                assert_eq!(id_part, "-1");
            }
            Error::Syntax(_) => panic!("Expected Id error"),
        }
    }

    #[test]
    fn test_try_from_multiple_dashes() {
        // Multiple dashes - only first one is used as separator
        let result = Hrid::try_from("MY-KIND-123");
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Id(hrid, id_part) => {
                assert_eq!(hrid, "MY-KIND-123");
                assert_eq!(id_part, "KIND-123");
            }
            Error::Syntax(_) => panic!("Expected Id error"),
        }
    }

    #[test]
    fn test_hrid_clone_and_eq() {
        let hrid1 = Hrid {
            kind: "URS".to_string(),
            id: 42,
        };
        let hrid2 = hrid1.clone();

        assert_eq!(hrid1, hrid2);
        assert_eq!(hrid1.kind, hrid2.kind);
        assert_eq!(hrid1.id, hrid2.id);
    }

    #[test]
    fn test_hrid_not_eq() {
        let hrid1 = Hrid {
            kind: "URS".to_string(),
            id: 42,
        };
        let hrid2 = Hrid {
            kind: "SYS".to_string(),
            id: 42,
        };
        let hrid3 = Hrid {
            kind: "URS".to_string(),
            id: 43,
        };

        assert_ne!(hrid1, hrid2);
        assert_ne!(hrid1, hrid3);
    }

    #[test]
    fn test_roundtrip_conversion() {
        let original = Hrid {
            kind: "TEST".to_string(),
            id: 123,
        };

        let as_string = format!("{original}");
        let parsed = Hrid::try_from(as_string.as_str()).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_error_display() {
        let syntax_error = Error::Syntax("bad-format".to_string());
        assert_eq!(format!("{syntax_error}"), "Invalid HRID format: bad-format");

        let id_error = Error::Id("URS-bad".to_string(), "bad".to_string());
        assert_eq!(
            format!("{id_error}"),
            "Invalid ID in HRID 'URS-bad': expected an integer, got bad"
        );
    }
}
