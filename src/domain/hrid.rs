use std::{fmt, str::FromStr};

use non_empty_string::NonEmptyString;

/// A human-readable identifier (HRID) for a requirement.
///
/// Format:
/// `{NAMESPACE*}-{KIND}-{ID}`, where:
/// - `NAMESPACE` is an optional sequence of non-empty segments (e.g.
///   `COMPONENT-SUBCOMPONENT`)
/// - `KIND` is a non-empty category string (e.g. `URS`, `SYS`)
/// - `ID` is a positive integer (e.g. `001`, `123`)
///
/// Examples: `URS-001`, `SYS-099`, `COMPONENT-SUBCOMPONENT-SYS-005`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hrid {
    namespace: Vec<NonEmptyString>,
    kind: NonEmptyString,
    id: usize,
}

/// Error returned when the provided string is empty
#[derive(Debug, thiserror::Error)]
#[error("found empty string")]
pub struct EmptyStringError;

impl Hrid {
    /// Create an HRID with no namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if `kind` is an empty string.
    pub fn new(kind: String, id: usize) -> Result<Self, EmptyStringError> {
        Self::new_with_namespace(Vec::new(), kind, id)
    }

    /// Create an HRID with the given namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if `kind` is empty or any namespace segment is empty.
    pub fn new_with_namespace(
        namespace: Vec<String>,
        kind: String,
        id: usize,
    ) -> Result<Self, EmptyStringError> {
        let kind = NonEmptyString::new(kind).map_err(|_| EmptyStringError)?;

        let validated_namespace = namespace
            .into_iter()
            .map(|s| NonEmptyString::new(s).map_err(|_| EmptyStringError))
            .collect::<Result<_, _>>()?;

        Ok(Self::new_with_namespace_unchecked(
            validated_namespace,
            kind,
            id,
        ))
    }

    /// Internal constructor that doesn't validate (for use after validation).
    const fn new_with_namespace_unchecked(
        namespace: Vec<NonEmptyString>,
        kind: NonEmptyString,
        id: usize,
    ) -> Self {
        Self {
            namespace,
            kind,
            id,
        }
    }

    /// Returns the namespace segments as strings.
    pub fn namespace(&self) -> Vec<&str> {
        self.namespace.iter().map(NonEmptyString::as_str).collect()
    }

    /// Returns the kind component as a string.
    #[must_use]
    pub fn kind(&self) -> &str {
        self.kind.as_str()
    }

    /// Returns the numeric ID component.
    #[must_use]
    pub const fn id(&self) -> usize {
        self.id
    }
}

impl fmt::Display for Hrid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let id_str = format!("{:03}", self.id);
        if self.namespace.is_empty() {
            write!(f, "{}-{}", self.kind, id_str)
        } else {
            let namespace_str = self
                .namespace
                .iter()
                .map(NonEmptyString::as_str)
                .collect::<Vec<_>>()
                .join("-");
            write!(f, "{}-{}-{}", namespace_str, self.kind, id_str)
        }
    }
}

/// Errors that can occur during HRID parsing or construction.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("Invalid HRID format: {0}")]
    Syntax(String),

    #[error("Invalid ID in HRID '{0}': expected an integer, got {1}")]
    Id(String, String),
}

impl FromStr for Hrid {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Early validation: check for empty string or malformed structure
        if s.is_empty()
            || s.starts_with('-')
            || s.ends_with('-')
            || s.contains("--")
            || !s.contains('-')
        {
            return Err(Error::Syntax(s.to_string()));
        }

        let parts: Vec<&str> = s.split('-').collect();

        // Must have at least KIND-ID (2 parts)
        if parts.len() < 2 {
            return Err(Error::Syntax(s.to_string()));
        }

        // Parse ID from the last part
        let id_str = parts[parts.len() - 1];
        let id = id_str
            .parse::<usize>()
            .map_err(|_| Error::Id(s.to_string(), id_str.to_string()))?;

        // Parse KIND from the second-to-last part
        let kind_str = parts[parts.len() - 2];
        let kind = NonEmptyString::from_str(kind_str).map_err(|_| Error::Syntax(s.to_string()))?;

        // Parse namespace from all remaining parts (if any)
        let namespace = if parts.len() > 2 {
            parts[..parts.len() - 2]
                .iter()
                .map(|&segment| NonEmptyString::from_str(segment))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| Error::Syntax(s.to_string()))?
        } else {
            Vec::new()
        };

        Ok(Self::new_with_namespace_unchecked(namespace, kind, id))
    }
}

impl TryFrom<&str> for Hrid {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hrid_creation_no_namespace() {
        let hrid = Hrid::new("URS".to_string(), 42).unwrap();
        assert!(hrid.namespace().is_empty());
        assert_eq!(hrid.kind(), "URS");
        assert_eq!(hrid.id(), 42);
    }

    #[test]
    fn hrid_creation_with_namespace() {
        let hrid = Hrid::new_with_namespace(
            vec!["COMPONENT".to_string(), "SUBCOMPONENT".to_string()],
            "SYS".to_string(),
            5,
        )
        .unwrap();

        assert_eq!(hrid.namespace(), vec!["COMPONENT", "SUBCOMPONENT"]);
        assert_eq!(hrid.kind(), "SYS");
        assert_eq!(hrid.id(), 5);
    }

    #[test]
    fn hrid_creation_empty_kind_fails() {
        let result = Hrid::new(String::new(), 42);
        assert!(matches!(result, Err(EmptyStringError)));
    }

    #[test]
    fn hrid_creation_empty_namespace_segment_fails() {
        let result = Hrid::new_with_namespace(
            vec!["COMPONENT".to_string(), String::new()],
            "SYS".to_string(),
            5,
        );
        assert!(matches!(result, Err(EmptyStringError)));
    }

    #[test]
    fn hrid_display_no_namespace() {
        let hrid = Hrid::new("SYS".to_string(), 1).unwrap();
        assert_eq!(format!("{hrid}"), "SYS-001");

        let hrid = Hrid::new("URS".to_string(), 42).unwrap();
        assert_eq!(format!("{hrid}"), "URS-042");

        let hrid = Hrid::new("TEST".to_string(), 999).unwrap();
        assert_eq!(format!("{hrid}"), "TEST-999");
    }

    #[test]
    fn hrid_display_with_namespace() {
        let hrid =
            Hrid::new_with_namespace(vec!["COMPONENT".to_string()], "SYS".to_string(), 5).unwrap();
        assert_eq!(format!("{hrid}"), "COMPONENT-SYS-005");

        let hrid = Hrid::new_with_namespace(
            vec!["COMPONENT".to_string(), "SUBCOMPONENT".to_string()],
            "SYS".to_string(),
            5,
        )
        .unwrap();
        assert_eq!(format!("{hrid}"), "COMPONENT-SUBCOMPONENT-SYS-005");

        let hrid = Hrid::new_with_namespace(
            vec!["A".to_string(), "B".to_string(), "C".to_string()],
            "REQ".to_string(),
            123,
        )
        .unwrap();
        assert_eq!(format!("{hrid}"), "A-B-C-REQ-123");
    }

    #[test]
    fn hrid_display_large_numbers() {
        let hrid = Hrid::new("BIG".to_string(), 1000).unwrap();
        assert_eq!(format!("{hrid}"), "BIG-1000");

        let hrid =
            Hrid::new_with_namespace(vec!["NS".to_string()], "HUGE".to_string(), 12345).unwrap();
        assert_eq!(format!("{hrid}"), "NS-HUGE-12345");
    }

    #[test]
    fn try_from_valid_no_namespace() {
        let hrid = Hrid::try_from("URS-001").unwrap();
        assert!(hrid.namespace().is_empty());
        assert_eq!(hrid.kind(), "URS");
        assert_eq!(hrid.id(), 1);

        let hrid = Hrid::try_from("SYS-042").unwrap();
        assert!(hrid.namespace().is_empty());
        assert_eq!(hrid.kind(), "SYS");
        assert_eq!(hrid.id(), 42);

        let hrid = Hrid::try_from("TEST-999").unwrap();
        assert!(hrid.namespace().is_empty());
        assert_eq!(hrid.kind(), "TEST");
        assert_eq!(hrid.id(), 999);
    }

    #[test]
    fn try_from_valid_with_namespace() {
        let hrid = Hrid::try_from("COMPONENT-SYS-005").unwrap();
        assert_eq!(hrid.namespace(), vec!["COMPONENT"]);
        assert_eq!(hrid.kind(), "SYS");
        assert_eq!(hrid.id(), 5);

        let hrid = Hrid::try_from("COMPONENT-SUBCOMPONENT-SYS-005").unwrap();
        assert_eq!(hrid.namespace(), vec!["COMPONENT", "SUBCOMPONENT"]);
        assert_eq!(hrid.kind(), "SYS");
        assert_eq!(hrid.id(), 5);

        let hrid = Hrid::try_from("A-B-C-REQ-123").unwrap();
        assert_eq!(hrid.namespace(), vec!["A", "B", "C"]);
        assert_eq!(hrid.kind(), "REQ");
        assert_eq!(hrid.id(), 123);
    }

    #[test]
    fn try_from_valid_no_leading_zeros() {
        let hrid = Hrid::try_from("URS-1").unwrap();
        assert!(hrid.namespace().is_empty());
        assert_eq!(hrid.kind(), "URS");
        assert_eq!(hrid.id(), 1);

        let hrid = Hrid::try_from("NS-SYS-42").unwrap();
        assert_eq!(hrid.namespace(), vec!["NS"]);
        assert_eq!(hrid.kind(), "SYS");
        assert_eq!(hrid.id(), 42);
    }

    #[test]
    fn try_from_valid_large_numbers() {
        let hrid = Hrid::try_from("BIG-1000").unwrap();
        assert!(hrid.namespace().is_empty());
        assert_eq!(hrid.kind(), "BIG");
        assert_eq!(hrid.id(), 1000);

        let hrid = Hrid::try_from("NS-HUGE-12345").unwrap();
        assert_eq!(hrid.namespace(), vec!["NS"]);
        assert_eq!(hrid.kind(), "HUGE");
        assert_eq!(hrid.id(), 12345);
    }

    #[test]
    fn try_from_invalid_no_dash() {
        let result = Hrid::try_from("URS001");
        assert_eq!(result, Err(Error::Syntax("URS001".to_string())));
    }

    #[test]
    fn try_from_invalid_empty_string() {
        let result = Hrid::try_from("");
        assert_eq!(result, Err(Error::Syntax(String::new())));
    }

    #[test]
    fn try_from_invalid_only_dash() {
        let result = Hrid::try_from("-");
        assert_eq!(result, Err(Error::Syntax("-".to_string())));
    }

    #[test]
    fn try_from_invalid_single_part() {
        let result = Hrid::try_from("JUSTONEWORD");
        assert_eq!(result, Err(Error::Syntax("JUSTONEWORD".to_string())));
    }

    #[test]
    fn try_from_invalid_non_numeric_id() {
        let result = Hrid::try_from("URS-abc");
        assert_eq!(
            result,
            Err(Error::Id("URS-abc".to_string(), "abc".to_string()))
        );

        let result = Hrid::try_from("NS-URS-abc");
        assert_eq!(
            result,
            Err(Error::Id("NS-URS-abc".to_string(), "abc".to_string()))
        );
    }

    #[test]
    fn try_from_invalid_mixed_id() {
        let result = Hrid::try_from("SYS-12abc");
        assert_eq!(
            result,
            Err(Error::Id("SYS-12abc".to_string(), "12abc".to_string()))
        );
    }

    #[test]
    fn try_from_invalid_negative_id() {
        let result = Hrid::try_from("URS--1");
        assert_eq!(result, Err(Error::Syntax("URS--1".to_string())));
    }

    #[test]
    fn try_from_empty_namespace_segment_fails() {
        let result = Hrid::try_from("-NS-SYS-001");
        assert!(result == Err(Error::Syntax("-NS-SYS-001".to_string())));

        let result = Hrid::try_from("NS--SYS-001");
        assert!(result == Err(Error::Syntax("NS--SYS-001".to_string())));
    }

    #[test]
    fn try_from_empty_kind_fails() {
        // This would actually be caught by consecutive dashes check now
        // but let's test a case where kind is empty due to structure
        let result = Hrid::try_from("-001");
        assert_eq!(result, Err(Error::Syntax("-001".to_string())));
    }

    #[test]
    fn hrid_clone_and_eq() {
        let hrid1 =
            Hrid::new_with_namespace(vec!["NS".to_string()], "URS".to_string(), 42).unwrap();
        let hrid2 = hrid1.clone();

        assert_eq!(hrid1, hrid2);
        assert_eq!(hrid1.namespace(), hrid2.namespace());
        assert_eq!(hrid1.kind(), hrid2.kind());
        assert_eq!(hrid1.id(), hrid2.id());
    }

    #[test]
    fn hrid_not_eq() {
        let hrid1 = Hrid::new("URS".to_string(), 42).unwrap();
        let hrid2 = Hrid::new("SYS".to_string(), 42).unwrap();
        let hrid3 = Hrid::new("URS".to_string(), 43).unwrap();
        let hrid4 =
            Hrid::new_with_namespace(vec!["NS".to_string()], "URS".to_string(), 42).unwrap();

        assert_ne!(hrid1, hrid2);
        assert_ne!(hrid1, hrid3);
        assert_ne!(hrid1, hrid4);
    }

    #[test]
    fn roundtrip_conversion_no_namespace() {
        let original = Hrid::new("TEST".to_string(), 123).unwrap();

        let as_string = format!("{original}");
        let parsed = Hrid::try_from(as_string.as_str()).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn roundtrip_conversion_with_namespace() {
        let original = Hrid::new_with_namespace(
            vec!["COMPONENT".to_string(), "SUBCOMPONENT".to_string()],
            "SYS".to_string(),
            5,
        )
        .unwrap();

        let as_string = format!("{original}");
        let parsed = Hrid::try_from(as_string.as_str()).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn error_display() {
        let syntax_error = Error::Syntax("bad-format".to_string());
        assert_eq!(format!("{syntax_error}"), "Invalid HRID format: bad-format");

        let id_error = Error::Id("URS-bad".to_string(), "bad".to_string());
        assert_eq!(
            format!("{id_error}"),
            "Invalid ID in HRID 'URS-bad': expected an integer, got bad"
        );
    }
}
