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
        let file = File::open(path.join(&hrid).with_extension("md")).map_err(LoadError::Io)?;
        let mut reader = BufReader::new(file);
        Self::read(&mut reader, hrid)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("failed to read from markdown")]
pub enum LoadError {
    Io(#[from] io::Error),
    Yaml(#[from] serde_yaml::Error),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(from = "FrontMatterVersion")]
struct FrontMatter {
    uuid: Uuid,
    created: DateTime<Utc>,
    tags: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "_version")]
enum FrontMatterVersion {
    #[serde(rename = "1")]
    V1(FrontMatter),
}

impl From<FrontMatterVersion> for FrontMatter {
    fn from(version: FrontMatterVersion) -> Self {
        match version {
            FrontMatterVersion::V1(front_matter) => front_matter,
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
                },
        } = req;

        let frontmatter = FrontMatter {
            uuid,
            created,
            tags,
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
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MarkdownRequirement;

    #[test]
    fn markdown_round_trip() {
        let hrid = "REQ-001";
        let expected = r"---
uuid: 12b3f5c5-b1a8-4aa8-a882-20ff1c2aab53
created: 2025-07-14T07:15:00Z
tags:
- tag1
- tag2
---

# The Title

This is a paragraph.
";

        let mut reader = std::io::Cursor::new(expected);
        let requirement = MarkdownRequirement::read(&mut reader, hrid.to_string()).unwrap();

        let mut bytes: Vec<u8> = vec![];

        requirement.write(&mut bytes);

        let actual = String::from_utf8(bytes).unwrap();

        assert_eq!(expected, &actual);
    }
}
