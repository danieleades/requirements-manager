use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::Path,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::requirement::{Content, Metadata};

use super::Requirement;

#[derive(Debug, Clone)]
pub struct MarkdownRequirement {
    frontmatter: FrontMatter,
    hrid: String,
    content: String,
}

impl MarkdownRequirement {
    fn write<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let frontmatter = serde_yaml::to_string(&self.frontmatter).expect("this must never fail");
        let result = format!("---\n{frontmatter}---\n{}\n", self.content);
        writer.write_all(result.as_bytes())
    }

    fn read<R: BufRead>(reader: &mut R, hrid: String) -> Result<Self, LoadError> {
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
    ///
    /// Note the path here is the path to the directory. The filename is determined by the HRID
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let file = File::create(path.join(&self.hrid).with_extension("md"))?;
        let mut writer = BufWriter::new(file);
        self.write(&mut writer)
    }

    /// Reads a requirement from the given file path.
    ///
    ///
    /// Note the path here is the path to the directory. The filename is determined by the HRID
    pub fn load(path: &Path, hrid: String) -> Result<Self, LoadError> {
        let file =
            File::open(path.join(&hrid).with_extension("md")).map_err(|io_error| match io_error
                .kind()
            {
                io::ErrorKind::NotFound => LoadError::NotFound,
                _ => LoadError::Io(io_error),
            })?;
        let mut reader = BufReader::new(file);
        Self::read(&mut reader, hrid)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("failed to read from markdown")]
pub enum LoadError {
    NotFound,
    Io(#[from] io::Error),
    Yaml(#[from] serde_yaml::Error),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(from = "FrontMatterVersion")]
#[serde(into = "FrontMatterVersion")]
struct FrontMatter {
    uuid: Uuid,
    created: DateTime<Utc>,
    tags: BTreeSet<String>,
    parents: Vec<Parent>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Parent {
    uuid: Uuid,
    fingerprint: String,
    hrid: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "_version")]
enum FrontMatterVersion {
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

impl From<FrontMatterVersion> for FrontMatter {
    fn from(version: FrontMatterVersion) -> Self {
        match version {
            FrontMatterVersion::V1 {
                uuid,
                created,
                tags,
                parents,
            } => Self {
                uuid,
                created,
                tags,
                parents,
            },
        }
    }
}

impl From<FrontMatter> for FrontMatterVersion {
    fn from(front_matter: FrontMatter) -> Self {
        let FrontMatter {
            uuid,
            created,
            tags,
            parents,
        } = front_matter;
        Self::V1 {
            uuid,
            created,
            tags,
            parents,
        }
    }
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

        let frontmatter = FrontMatter {
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
            frontmatter:
                FrontMatter {
                    uuid,
                    created,
                    tags,
                    parents,
                },
            hrid,
            content,
        } = req;
        Self {
            content: Content { content, tags },
            metadata: Metadata {
                uuid,
                hrid,
                created,
                parents: parents
                    .into_iter()
                    .map(
                        |Parent {
                             uuid,
                             fingerprint,
                             hrid,
                         }| (uuid, super::Parent { hrid, fingerprint }),
                    )
                    .collect(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Parent;
    use super::*;
    use chrono::TimeZone;
    use std::io::Cursor;
    use tempfile::TempDir;

    fn create_test_frontmatter() -> FrontMatter {
        let uuid = Uuid::parse_str("12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53").unwrap();
        let created = Utc.with_ymd_and_hms(2025, 7, 14, 7, 15, 0).unwrap();
        let tags = BTreeSet::from(["tag1".to_string(), "tag2".to_string()]);
        let parents = vec![Parent {
            uuid: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            fingerprint: "fingerprint1".to_string(),
            hrid: "REQ-PARENT-001".to_string(),
        }];
        FrontMatter {
            uuid,
            created,
            tags,
            parents,
        }
    }

    #[test]
    fn test_markdown_round_trip() {
        let hrid = "REQ-001";
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
        let requirement = MarkdownRequirement::read(&mut reader, hrid.to_string()).unwrap();

        let mut bytes: Vec<u8> = vec![];
        requirement.write(&mut bytes).unwrap();

        let actual = String::from_utf8(bytes).unwrap();
        assert_eq!(expected, &actual);
    }

    #[test]
    fn test_markdown_minimal_content() {
        let hrid = "REQ-002";
        let content = r"---
_version: '1'
uuid: 12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53
created: 2025-07-14T07:15:00Z
---
Just content
";

        let mut reader = Cursor::new(content);
        let requirement = MarkdownRequirement::read(&mut reader, hrid.to_string()).unwrap();

        assert_eq!(requirement.hrid, hrid);
        assert_eq!(requirement.content, "Just content");
        assert!(requirement.frontmatter.tags.is_empty());
        assert!(requirement.frontmatter.parents.is_empty());
    }

    #[test]
    fn test_empty_content() {
        let hrid = "REQ-003";
        let content = r"---
_version: '1'
uuid: 12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53
created: 2025-07-14T07:15:00Z
---
";

        let mut reader = Cursor::new(content);
        let requirement = MarkdownRequirement::read(&mut reader, hrid.to_string()).unwrap();

        assert_eq!(requirement.content, "");
    }

    #[test]
    fn test_multiline_content() {
        let hrid = "REQ-004";
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
        let requirement = MarkdownRequirement::read(&mut reader, hrid.to_string()).unwrap();

        assert_eq!(requirement.content, "Line 1\nLine 2\n\nLine 4");
    }

    #[test]
    fn test_invalid_frontmatter_start() {
        let hrid = "REQ-005";
        let content = "invalid frontmatter";

        let mut reader = Cursor::new(content);
        let result = MarkdownRequirement::read(&mut reader, hrid.to_string());

        assert!(result.is_err());
    }

    #[test]
    fn test_missing_frontmatter_end() {
        let hrid = "REQ-006";
        let content = r"---
uuid: 12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53
created: 2025-07-14T07:15:00Z
This should be content but there's no closing ---";

        let mut reader = Cursor::new(content);
        let result = MarkdownRequirement::read(&mut reader, hrid.to_string());

        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_yaml() {
        let hrid = "REQ-007";
        let content = r"---
invalid: yaml: structure:
created: not-a-date
---
Content";

        let mut reader = Cursor::new(content);
        let result = MarkdownRequirement::read(&mut reader, hrid.to_string());

        assert!(matches!(result, Err(LoadError::Yaml(_))));
    }

    #[test]
    fn test_empty_input() {
        let hrid = "REQ-008";
        let content = "";

        let mut reader = Cursor::new(content);
        let result = MarkdownRequirement::read(&mut reader, hrid.to_string());

        assert!(result.is_err());
    }

    #[test]
    fn test_write_success() {
        let frontmatter = create_test_frontmatter();
        let requirement = MarkdownRequirement {
            frontmatter,
            hrid: "REQ-009".to_string(),
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
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let frontmatter = create_test_frontmatter();
        let hrid = "REQ-010".to_string();
        let content = "Saved content".to_string();

        let requirement = MarkdownRequirement {
            frontmatter: frontmatter.clone(),
            hrid: hrid.clone(),
            content: content.clone(),
        };

        // Test save
        let save_result = requirement.save(temp_dir.path());
        assert!(save_result.is_ok());

        // Test load
        let loaded_requirement = MarkdownRequirement::load(temp_dir.path(), hrid.clone()).unwrap();
        assert_eq!(loaded_requirement.hrid, hrid);
        assert_eq!(loaded_requirement.content, content);
        assert_eq!(loaded_requirement.frontmatter, frontmatter);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let result = MarkdownRequirement::load(temp_dir.path(), "NONEXISTENT".to_string());
        assert!(matches!(result, Err(LoadError::NotFound)));
    }

    #[test]
    fn test_frontmatter_version_conversion() {
        let uuid = Uuid::parse_str("12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53").unwrap();
        let created = Utc.with_ymd_and_hms(2025, 7, 14, 7, 15, 0).unwrap();
        let tags = BTreeSet::from(["tag1".to_owned()]);
        let parents = vec![Parent {
            uuid: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            fingerprint: "fp1".to_string(),
            hrid: "REQ-P1".to_string(),
        }];

        let frontmatter = FrontMatter {
            uuid,
            created,
            tags,
            parents,
        };
        let version: FrontMatterVersion = frontmatter.clone().into();
        let back_to_frontmatter: FrontMatter = version.into();

        assert_eq!(frontmatter, back_to_frontmatter);
    }

    #[test]
    fn test_parent_creation() {
        let uuid = Uuid::parse_str("12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53").unwrap();
        let fingerprint = "test-fingerprint".to_string();
        let hrid = "REQ-PARENT".to_string();

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
    fn test_content_with_triple_dashes() {
        let hrid = "REQ-012";
        let content = r"---
_version: '1'
uuid: 12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53
created: 2025-07-14T07:15:00Z
---
This content has --- in it
And more --- here
";

        let mut reader = Cursor::new(content);
        let requirement = MarkdownRequirement::read(&mut reader, hrid.to_string()).unwrap();

        assert_eq!(
            requirement.content,
            "This content has --- in it\nAnd more --- here"
        );
    }

    #[test]
    fn test_frontmatter_with_special_characters() {
        let hrid = "REQ-013";
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
        let requirement = MarkdownRequirement::read(&mut reader, hrid.to_string()).unwrap();

        assert!(requirement.frontmatter.tags.contains("tag with spaces"));
        assert!(requirement.frontmatter.tags.contains("tag-with-dashes"));
        assert!(
            requirement
                .frontmatter
                .tags
                .contains("tag_with_underscores")
        );
    }
}
