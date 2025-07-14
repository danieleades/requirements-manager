use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::Path,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain;

pub struct Requirement {
    frontmatter: FrontMatter,
    hrid: String,
    content: String,
}

impl Requirement {
    fn to_markdown<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let frontmatter = serde_yaml::to_string(&self.frontmatter).expect("this must never fail");
        let result = format!("---\n{frontmatter}---\n{}\n", self.content);
        writer.write_all(result.as_bytes())
    }

    #[cfg(test)]
    fn to_markdown_string(&self) -> String {
        let mut bytes = vec![];
        self.to_markdown(&mut bytes).unwrap();
        // safety: this doesn't need to be checked since this is trusted input
        String::from_utf8(bytes).expect("parsing trusted input must never fail")
    }

    fn from_markdown<R: BufRead>(reader: &mut R, hrid: String) -> Result<Self, FromMarkdownError> {
        let mut lines = reader.lines();

        // Ensure frontmatter starts correctly
        let first_line = lines
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "Empty input"))?
            .map_err(FromMarkdownError::from)?;

        if first_line.trim() != "---" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Expected frontmatter starting with '---'",
            )
            .into());
        }

        // Collect lines until next '---'
        let frontmatter_lines: Vec<String> = lines
            .by_ref()
            .map_while(|line| match line {
                Ok(content) if content.trim() == "---" => None,
                Ok(content) => Some(Ok(content)),
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<_, _>>()?;

        let frontmatter_str = frontmatter_lines.join("\n");

        // The rest of the lines are Markdown content
        let content = lines.collect::<Result<Vec<_>, _>>()?.join("\n");

        let front: FrontMatter = serde_yaml::from_str(&frontmatter_str)?;

        Ok(Self {
            frontmatter: front,
            hrid,
            content,
        })
    }

    #[cfg(test)]
    fn from_markdown_str(s: &str, hrid: String) -> Result<Self, serde_yaml::Error> {
        let cursor = io::Cursor::new(s);
        let mut reader = BufReader::new(cursor);
        Self::from_markdown(&mut reader, hrid).map_err(|e| {
            match e {
                // reading from a str should never fail
                FromMarkdownError::Io(_) => unreachable!(),
                FromMarkdownError::Yaml(error) => error,
            }
        })
    }

    /// Writes the requirement to the given file path.
    /// Creates the file if it doesn't exist, or overwrites it if it does.
    pub fn to_file(&self, path: &Path) -> io::Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        self.to_markdown(&mut writer)
    }

    /// Reads a requirement from the given file path.
    pub fn from_file(path: &Path, hrid: String) -> Result<Self, FromMarkdownError> {
        let file = File::open(path).map_err(FromMarkdownError::Io)?;
        let mut reader = BufReader::new(file);
        Self::from_markdown(&mut reader, hrid)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("failed to read from markdown")]
pub enum FromMarkdownError {
    Io(#[from] io::Error),
    Yaml(#[from] serde_yaml::Error),
}

#[derive(Debug, Serialize, Deserialize)]
struct FrontMatter {
    uuid: Uuid,
    created: DateTime<Utc>,
    tags: BTreeSet<String>,
}

impl From<domain::Requirement> for Requirement {
    fn from(req: domain::Requirement) -> Self {
        let uuid = req.uuid();
        let hrid = req.hrid().to_string();
        let created = req.created();
        let tags = req.tags().iter().cloned().collect();
        let content = req.content().to_string();

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

impl From<Requirement> for domain::Requirement {
    fn from(req: Requirement) -> Self {
        let Requirement {
            frontmatter:
                FrontMatter {
                    uuid,
                    created,
                    tags,
                },
            hrid,
            content,
        } = req;
        Self::from_parts(uuid, created, hrid, content, tags.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::Requirement;

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

        let requirement = Requirement::from_markdown_str(expected, hrid.to_string()).unwrap();

        let actual = requirement.to_markdown_string();

        assert_eq!(expected, &actual);
    }
}
