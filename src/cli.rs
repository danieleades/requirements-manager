use std::path::PathBuf;

use crate::storage::Directory;

#[derive(Debug, clap::Parser)]
pub enum Command {
    /// Add a new requirement
    Add(Add),
}

impl Command {
    pub fn run(self) {
        match self {
            Self::Add(command) => command.run(),
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
