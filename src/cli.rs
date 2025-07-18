use std::path::PathBuf;

use clap::ArgAction;
use tracing::instrument;

use crate::storage::{Directory, Tree};

#[derive(Debug, clap::Parser)]
#[command(version, about)]
pub struct Cli {
    /// Verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub fn run(self) {
        Self::setup_logging(self.verbose);

        self.command.run();
    }

    fn setup_logging(verbosity: u8) {
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

        let level = match verbosity {
            0 => tracing::Level::WARN,
            1 => tracing::Level::INFO,
            2 => tracing::Level::DEBUG,
            _ => tracing::Level::TRACE,
        };

        let filter = tracing_subscriber::EnvFilter::from_default_env().add_directive(level.into());

        let fmt_layer = tracing_subscriber::fmt::layer()
            .pretty()
            .with_target(false)
            .with_thread_names(false)
            .with_line_number(true);

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    }
}

#[derive(Debug, clap::Parser)]
pub enum Command {
    /// Add a new requirement
    Add(Add),

    /// Create a link between two requirements
    ///
    /// Links are parent-child relationships.
    Link(Link),

    /// Correct parent HRIDs
    Clean(Clean),
}

impl Command {
    fn run(self) {
        match self {
            Self::Add(command) => command.run(),
            Self::Link(command) => command.run(),
            Self::Clean(command) => command.run(),
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
    #[instrument]
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
    #[instrument]
    fn run(self) {
        let directory = Directory::open(self.root);
        directory.link_requirement(self.child, self.parent);
    }
}

#[derive(Debug, clap::Parser)]
pub struct Clean {
    /// The path to the root of the requirements directory
    #[arg(short, long, default_value = ".")]
    root: PathBuf,
}

impl Clean {
    #[instrument]
    fn run(self) {
        let mut tree = Tree::load_all(self.root);
        tree.update_hrids();
    }
}
