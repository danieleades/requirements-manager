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
    pub namespace: Vec<NonEmptyString>,
    pub kind: NonEmptyString,
    pub id: usize,
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
    #[must_use]
    pub const fn new(kind: NonEmptyString, id: usize) -> Self {
        Self::new_with_namespace_unchecked(Vec::new(), kind, id)
    }

    /// Create an HRID with the given namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if `kind` is empty or any namespace segment is empty.
    pub fn new_with_namespace(
        namespace: Vec<String>,
        kind: NonEmptyString,
        id: usize,
    ) -> Result<Self, EmptyStringError> {
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
    use test_case::test_case;

    use super::*;

    fn nes(s: &str) -> NonEmptyString {
        NonEmptyString::new(s.to_string()).unwrap()
    }

    #[test_case("URS", 42 => "URS-042")]
    #[test_case("SYS", 1 => "SYS-001")]
    #[test_case("TEST", 999 => "TEST-999")]
    fn display_no_namespace(kind: &str, id: usize) -> String {
        Hrid::new(nes(kind), id).to_string()
    }

    #[test_case(vec!["COMPONENT"], "SYS", 5 => "COMPONENT-SYS-005")]
    #[test_case(vec!["COMPONENT", "SUB"], "SYS", 5 => "COMPONENT-SUB-SYS-005")]
    #[test_case(vec!["A", "B", "C"], "REQ", 123 => "A-B-C-REQ-123")]
    fn display_with_namespace(namespace: Vec<&str>, kind: &str, id: usize) -> String {
        let ns = namespace
            .into_iter()
            .map(std::string::ToString::to_string)
            .collect();
        let hrid = Hrid::new_with_namespace(ns, nes(kind), id).unwrap();
        hrid.to_string()
    }

    #[test_case("URS-001", &[], "URS", 1)]
    #[test_case("NS-SYS-42", &["NS"], "SYS", 42)]
    #[test_case("A-B-C-REQ-123", &["A", "B", "C"], "REQ", 123)]
    fn parse_valid(input: &str, expected_ns: &[&str], expected_kind: &str, expected_id: usize) {
        let hrid = Hrid::try_from(input).unwrap();
        assert_eq!(hrid.namespace(), expected_ns);
        assert_eq!(hrid.kind(), expected_kind);
        assert_eq!(hrid.id(), expected_id);
    }

    #[test]
    fn equality_and_clone() {
        let h1 = Hrid::new_with_namespace(vec!["NS".into()], nes("URS"), 42).unwrap();
        let h2 = h1.clone();
        assert_eq!(h1, h2);

        let h3 = Hrid::new(nes("URS"), 43);
        assert_ne!(h1, h3);

        let h4 = Hrid::new(nes("SYS"), 42);
        assert_ne!(h1, h4);

        let h5 = Hrid::new_with_namespace(vec![], nes("URS"), 42).unwrap();
        assert_ne!(h1, h5);
    }

    #[test]
    fn roundtrip_display_and_parse() {
        let original =
            Hrid::new_with_namespace(vec!["COMPONENT".into(), "SUB".into()], nes("SYS"), 5)
                .unwrap();
        let roundtripped = Hrid::try_from(original.to_string().as_str()).unwrap();
        assert_eq!(original, roundtripped);
    }

    #[test]
    fn error_display() {
        let e1 = Error::Syntax("bad".into());
        assert_eq!(e1.to_string(), "Invalid HRID format: bad");

        let e2 = Error::Id("URS-abc".into(), "abc".into());
        assert_eq!(
            e2.to_string(),
            "Invalid ID in HRID 'URS-abc': expected an integer, got abc"
        );
    }
}
