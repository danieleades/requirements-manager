pub mod markdown {
    use std::str::FromStr;

    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    use crate::domain;

    pub struct Requirement {
        body: Body,
        frontmatter: FrontMatter,
    }

    pub struct RequirementRef<'a> {
        body: BodyRef<'a>,
        frontmatter: FrontMatter,
    }

    impl<'a> RequirementRef<'a> {
        pub fn new(
            uuid: Uuid,
            req: &'a domain::Requirement,
            parents: impl IntoIterator<Item = (Uuid, domain::requirement::Parent)>,
        ) -> Self {
            let parents = parents
                .into_iter()
                .map(
                    |(parent_uuid, domain::requirement::Parent { hrid, fingerprint })| v1::Parent {
                        uuid: parent_uuid,
                        fingerprint,
                        hrid: hrid.to_string(),
                    },
                )
                .collect();
            let frontmatter = FrontMatter::V1(v1::FrontMatter {
                uuid,
                parents,
                created: req.created(),
            });
            let body = BodyRef {
                content: req.content(),
            };
            Self { body, frontmatter }
        }
    }

    impl<'a> std::fmt::Display for RequirementRef<'a> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            // Serialize frontmatter to YAML
            let yaml = serde_yaml::to_string(&self.frontmatter).map_err(|_| std::fmt::Error)?;

            write!(f, "---\n{}---\n\n{}", yaml, self.body.content)
        }
    }

    impl FromStr for Requirement {
        type Err = FromStrError;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            todo!()
        }
    }

    #[derive(Debug, thiserror::Error)]
    pub enum FromStrError {
        #[error("Failed to parse YAML frontmatter: {0}")]
        YamlError(#[from] serde_yaml::Error),

        #[error("Invalid markdown structure")]
        InvalidStructure,

        #[error("Missing frontmatter")]
        MissingFrontmatter,
    }

    struct Body {
        content: String,
    }

    struct BodyRef<'a> {
        content: &'a str,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(tag = "_version")]
    enum FrontMatter {
        #[serde(rename = "1")]
        V1(v1::FrontMatter),
    }

    mod v1 {
        use chrono::{DateTime, Utc};
        use serde::{Deserialize, Serialize};
        use uuid::Uuid;

        use crate::domain::Fingerprint;

        #[derive(Serialize, Deserialize)]
        pub struct FrontMatter {
            pub(super) uuid: Uuid,
            pub(super) parents: Vec<Parent>,
            pub(super) created: DateTime<Utc>,
        }

        #[derive(Debug, Serialize, Deserialize)]
        pub struct Parent {
            pub(super) uuid: Uuid,
            pub(super) fingerprint: Fingerprint,
            pub(super) hrid: String,
        }
    }
}
