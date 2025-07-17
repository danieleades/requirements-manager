use std::path::PathBuf;

use crate::storage::Directory;

#[derive(Debug, clap::Parser)]
pub enum Command {
    /// Add a new requirement
    Add(Add),

    /// Create a link between two requirements
    ///
    /// Links are parent-child relationships.
    Link(Link),
}

impl Command {
    pub fn run(self) {
        match self {
            Self::Add(command) => command.run(),
            Self::Link(command) => command.run(),
        }
    }
}

#[derive(Debug, clap::Parser)]
pub struct Add {
    /// The kind of requirement to create.
    ///
    /// eg. 'USR' or 'SYS'.
    kind: String,

    /// The path to the root of the requirements directory
    #[arg(short, long, default_value = ".")]
    root: PathBuf,
}

impl Add {
    fn run(self) {
        let directory = Directory::open(self.root);
        directory.add_requirement(&self.kind);
    }
}

#[derive(Debug, clap::Parser)]
pub struct Link {
    /// The human-readable ID of the child document
    child: String,

    /// The human-readable ID of the parent document
    parent: String,

    /// The path to the root of the requirements directory
    #[arg(short, long, default_value = ".")]
    root: PathBuf,
}

impl Link {
    fn run(self) {
        let directory = Directory::open(self.root);
        directory.link_requirement(self.child, self.parent);
    }
}
