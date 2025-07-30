use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::Path,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Requirement;
use crate::domain::{
    requirement::{Content, Metadata},
    Hrid,
};

#[derive(Debug, Clone)]
pub struct MarkdownRequirement {
    frontmatter: FrontMatter,
    hrid: Hrid,
    content: String,
}

impl MarkdownRequirement {
    fn write<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let frontmatter = serde_yaml::to_string(&self.frontmatter).expect("this must never fail");
        let result = format!("---\n{frontmatter}---\n{}\n", self.content);
        writer.write_all(result.as_bytes())
    }

    fn read<R: BufRead>(reader: &mut R, hrid: Hrid) -> Result<Self, LoadError> {
        let mut lines = reader.lines();

        // Ensure frontmatter starts correctly
        let first_line = lines
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "Empty input"))?
            .map_err(LoadError::from)?;

        if first_line.trim() != "---" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Expected frontmatter starting with '---'",
            )
            .into());
        }

        // Collect lines until next '---'
        let frontmatter = lines
            .by_ref()
            .map_while(|line| match line {
                Ok(content) if content.trim() == "---" => None,
                Ok(content) => Some(Ok(content)),
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");

        // The rest of the lines are Markdown content
        let content = lines.collect::<Result<Vec<_>, _>>()?.join("\n");

        let front: FrontMatter = serde_yaml::from_str(&frontmatter)?;

        Ok(Self {
            frontmatter: front,
            hrid,
            content,
        })
    }

    /// Writes the requirement to the given file path.
    /// Creates the file if it doesn't exist, or overwrites it if it does.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        self.write(&mut writer)
    }

    /// Reads a requirement from the given file path.
    pub fn load(path: &Path, hrid: Hrid) -> Result<Self, LoadError> {
        let file = File::open(path).map_err(|io_error| match io_error.kind() {
            io::ErrorKind::NotFound => LoadError::NotFound,
            _ => LoadError::Io(io_error),
        })?;
        let mut reader = BufReader::new(file);
        Self::read(&mut reader, hrid)
    }

    #[cfg(test)]
    const fn tags(&self) -> &BTreeSet<String> {
        match &self.frontmatter {
            FrontMatter::V1 { tags, .. } => tags,
        }
    }

    #[cfg(test)]
    const fn parents(&self) -> &Vec<Parent> {
        match &self.frontmatter {
            FrontMatter::V1 { parents, .. } => parents,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("failed to read from markdown")]
pub enum LoadError {
    NotFound,
    Io(#[from] io::Error),
    Yaml(#[from] serde_yaml::Error),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Parent {
    uuid: Uuid,
    fingerprint: String,
    #[serde(
        serialize_with = "hrid_as_string",
        deserialize_with = "hrid_from_string"
    )]
    hrid: Hrid,
}

pub fn hrid_as_string<S>(hrid: &Hrid, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&hrid.to_string())
}

pub fn hrid_from_string<'de, D>(deserializer: D) -> Result<Hrid, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Hrid::try_from(s.as_str()).map_err(serde::de::Error::custom)
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "_version")]
enum FrontMatter {
    #[serde(rename = "1")]
    V1 {
        uuid: Uuid,
        created: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        tags: BTreeSet<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        parents: Vec<Parent>,
    },
}

impl From<Requirement> for MarkdownRequirement {
    fn from(req: Requirement) -> Self {
        let Requirement {
            content: Content { content, tags },
            metadata:
                Metadata {
                    uuid,
                    hrid,
                    created,
                    parents,
                },
        } = req;

        let frontmatter = FrontMatter::V1 {
            uuid,
            created,
            tags,
            parents: parents
                .into_iter()
                .map(|(uuid, super::Parent { hrid, fingerprint })| Parent {
                    uuid,
                    fingerprint,
                    hrid,
                })
                .collect(),
        };

        Self {
            frontmatter,
            hrid,
            content,
        }
    }
}

impl From<MarkdownRequirement> for Requirement {
    fn from(req: MarkdownRequirement) -> Self {
        let MarkdownRequirement {
            hrid,
            frontmatter:
                FrontMatter::V1 {
                    uuid,
                    created,
                    tags,
                    parents,
                },
            content,
        } = req;

        let parent_map = parents
            .into_iter()
            .map(|parent| {
                let Parent {
                    uuid,
                    fingerprint,
                    hrid: parent_hrid,
                } = parent;
                (
                    uuid,
                    super::Parent {
                        hrid: parent_hrid,
                        fingerprint,
                    },
                )
            })
            .collect();

        Self {
            content: Content { content, tags },
            metadata: Metadata {
                uuid,
                hrid,
                created,
                parents: parent_map,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use chrono::TimeZone;
    use tempfile::TempDir;

    use super::{Parent, *};

    fn create_test_frontmatter() -> FrontMatter {
        let uuid = Uuid::parse_str("12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53").unwrap();
        let created = Utc.with_ymd_and_hms(2025, 7, 14, 7, 15, 0).unwrap();
        let tags = BTreeSet::from(["tag1".to_string(), "tag2".to_string()]);
        let parents = vec![Parent {
            uuid: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            fingerprint: "fingerprint1".to_string(),
            hrid: "REQ-PARENT-001".parse().unwrap(),
        }];
        FrontMatter::V1 {
            uuid,
            created,
            tags,
            parents,
        }
    }

    #[test]
    fn markdown_round_trip() {
        let hrid = "REQ-001".parse().unwrap();
        let expected = r"---
_version: '1'
uuid: 12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53
created: 2025-07-14T07:15:00Z
tags:
- tag1
- tag2
parents:
- uuid: 550e8400-e29b-41d4-a716-446655440000
  fingerprint: fingerprint1
  hrid: REQ-PARENT-001
---

# The Title

This is a paragraph.
";

        let mut reader = Cursor::new(expected);
        let requirement = MarkdownRequirement::read(&mut reader, hrid).unwrap();

        let mut bytes: Vec<u8> = vec![];
        requirement.write(&mut bytes).unwrap();

        let actual = String::from_utf8(bytes).unwrap();
        assert_eq!(expected, &actual);
    }

    #[test]
    fn markdown_minimal_content() {
        let hrid = Hrid::new("REQ".to_string(), 1).unwrap();
        let content = r"---
_version: '1'
uuid: 12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53
created: 2025-07-14T07:15:00Z
---
Just content
";

        let mut reader = Cursor::new(content);
        let requirement = MarkdownRequirement::read(&mut reader, hrid.clone()).unwrap();

        assert_eq!(requirement.hrid, hrid);
        assert_eq!(requirement.content, "Just content");
        assert!(requirement.tags().is_empty());
        assert!(requirement.parents().is_empty());
    }

    #[test]
    fn empty_content() {
        let hrid = Hrid::new("REQ".to_string(), 1).unwrap();
        let content = r"---
_version: '1'
uuid: 12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53
created: 2025-07-14T07:15:00Z
---
";

        let mut reader = Cursor::new(content);
        let requirement = MarkdownRequirement::read(&mut reader, hrid).unwrap();

        assert_eq!(requirement.content, "");
    }

    #[test]
    fn multiline_content() {
        let hrid = Hrid::new("REQ".to_string(), 1).unwrap();
        let content = r"---
_version: '1'
uuid: 12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53
created: 2025-07-14T07:15:00Z
---
Line 1
Line 2

Line 4
";

        let mut reader = Cursor::new(content);
        let requirement = MarkdownRequirement::read(&mut reader, hrid).unwrap();

        assert_eq!(requirement.content, "Line 1\nLine 2\n\nLine 4");
    }

    #[test]
    fn invalid_frontmatter_start() {
        let hrid = Hrid::new("REQ".to_string(), 1).unwrap();
        let content = "invalid frontmatter";

        let mut reader = Cursor::new(content);
        let result = MarkdownRequirement::read(&mut reader, hrid);

        assert!(result.is_err());
    }

    #[test]
    fn missing_frontmatter_end() {
        let hrid = Hrid::new("REQ".to_string(), 1).unwrap();
        let content = r"---
uuid: 12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53
created: 2025-07-14T07:15:00Z
This should be content but there's no closing ---";

        let mut reader = Cursor::new(content);
        let result = MarkdownRequirement::read(&mut reader, hrid);

        assert!(result.is_err());
    }

    #[test]
    fn invalid_yaml() {
        let hrid = Hrid::new("REQ".to_string(), 1).unwrap();
        let content = r"---
invalid: yaml: structure:
created: not-a-date
---
Content";

        let mut reader = Cursor::new(content);
        let result = MarkdownRequirement::read(&mut reader, hrid);

        assert!(matches!(result, Err(LoadError::Yaml(_))));
    }

    #[test]
    fn empty_input() {
        let hrid = Hrid::new("REQ".to_string(), 1).unwrap();
        let content = "";

        let mut reader = Cursor::new(content);
        let result = MarkdownRequirement::read(&mut reader, hrid);

        assert!(result.is_err());
    }

    #[test]
    fn write_success() {
        let frontmatter = create_test_frontmatter();
        let requirement = MarkdownRequirement {
            frontmatter,
            hrid: Hrid::new("REQ".to_string(), 1).unwrap(),
            content: "Test content".to_string(),
        };

        let mut buffer = Vec::new();
        let result = requirement.write(&mut buffer);

        assert!(result.is_ok());
        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("---"));
        assert!(output.contains("Test content"));
    }

    #[test]
    fn save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let frontmatter = create_test_frontmatter();
        let hrid = Hrid::new("REQ".to_string(), 1).unwrap();
        let content = "Saved content".to_string();

        let requirement = MarkdownRequirement {
            frontmatter: frontmatter.clone(),
            hrid: hrid.clone(),
            content: content.clone(),
        };

        // Create the file path (directory + filename)
        let file_path = temp_dir.path().join("REQ-001.md");

        // Test save
        let save_result = requirement.save(&file_path);
        assert!(save_result.is_ok());

        // Test load
        let loaded_requirement = MarkdownRequirement::load(&file_path, hrid.clone()).unwrap();
        assert_eq!(loaded_requirement.hrid, hrid);
        assert_eq!(loaded_requirement.content, content);
        assert_eq!(loaded_requirement.frontmatter, frontmatter);
    }

    #[test]
    fn load_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let result = MarkdownRequirement::load(
            &temp_dir.path().join("missing.md"),
            Hrid::new("REQ".to_string(), 1).unwrap(),
        );
        assert!(matches!(result, Err(LoadError::NotFound)));
    }

    #[test]
    fn frontmatter_version_conversion() {
        let uuid = Uuid::parse_str("12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53").unwrap();
        let created = Utc.with_ymd_and_hms(2025, 7, 14, 7, 15, 0).unwrap();
        let tags = BTreeSet::from(["tag1".to_owned()]);
        let parents = vec![Parent {
            uuid: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            fingerprint: "fp1".to_string(),
            hrid: Hrid::new("REQ".to_string(), 1).unwrap(),
        }];

        let frontmatter = FrontMatter::V1 {
            uuid,
            created,
            tags,
            parents,
        };
        let version: FrontMatter = frontmatter.clone();
        let back_to_frontmatter: FrontMatter = version;

        assert_eq!(frontmatter, back_to_frontmatter);
    }

    #[test]
    fn parent_creation() {
        let uuid = Uuid::parse_str("12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53").unwrap();
        let fingerprint = "test-fingerprint".to_string();
        let hrid = Hrid::new("REQ".to_string(), 1).unwrap();

        let parent = Parent {
            uuid,
            fingerprint: fingerprint.clone(),
            hrid: hrid.clone(),
        };

        assert_eq!(parent.uuid, uuid);
        assert_eq!(parent.fingerprint, fingerprint);
        assert_eq!(parent.hrid, hrid);
    }

    #[test]
    fn content_with_triple_dashes() {
        let hrid = Hrid::new("REQ".to_string(), 1).unwrap();
        let content = r"---
_version: '1'
uuid: 12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53
created: 2025-07-14T07:15:00Z
---
This content has --- in it
And more --- here
";

        let mut reader = Cursor::new(content);
        let requirement = MarkdownRequirement::read(&mut reader, hrid).unwrap();

        assert_eq!(
            requirement.content,
            "This content has --- in it\nAnd more --- here"
        );
    }

    #[test]
    fn frontmatter_with_special_characters() {
        let hrid = Hrid::new("REQ".to_string(), 1).unwrap();
        let content = r#"---
_version: '1'
uuid: 12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53
created: 2025-07-14T07:15:00Z
tags:
- "tag with spaces"
- "tag-with-dashes"
- "tag_with_underscores"
---
Content here
"#;

        let mut reader = Cursor::new(content);
        let requirement = MarkdownRequirement::read(&mut reader, hrid).unwrap();

        assert!(requirement.tags().contains("tag with spaces"));
        assert!(requirement.tags().contains("tag-with-dashes"));
        assert!(requirement.tags().contains("tag_with_underscores"));
    }
}
