use std::path::PathBuf;

use clap::ArgAction;
use requiem::{Directory, Hrid};
use tracing::instrument;

#[derive(Debug, clap::Parser)]
#[command(version, about)]
pub struct Cli {
    /// Verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = ArgAction::Count, global=true)]
    verbose: u8,

    /// The path to the root of the requirements directory
    #[arg(short, long, default_value = ".", global = true)]
    root: PathBuf,

    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub fn run(self) -> anyhow::Result<()> {
        Self::setup_logging(self.verbose);

        self.command.run(self.root)
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
            //.pretty()
            .with_target(false)
            .with_thread_names(false)
            .with_line_number(false);

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
    Clean,
}

impl Command {
    fn run(self, root: PathBuf) -> anyhow::Result<()> {
        match self {
            Self::Add(command) => command.run(root)?,
            Self::Link(command) => command.run(root)?,
            Self::Clean => Clean::run(root)?,
        }
        Ok(())
    }
}

#[derive(Debug, clap::Parser)]
pub struct Add {
    /// The kind of requirement to create.
    ///
    /// eg. 'USR' or 'SYS'.
    kind: String,

    /// The human-readable IDs of the parent requirements.
    #[clap(long, short, value_delimiter = ',')]
    parent: Vec<Hrid>,
}

impl Add {
    #[instrument]
    fn run(self, root: PathBuf) -> anyhow::Result<()> {
        let mut directory = Directory::new(root).load_all()?;
        let requirement = directory.add_requirement(self.kind)?;

        for parent in self.parent {
            // TODO: the linkage should be done before the requirement is saved by the
            // 'add_requirement' method to avoid unnecessary IO.
            directory.link_requirement(requirement.hrid().clone(), parent)?;
        }

        println!("Added requirement {}", requirement.hrid());
        Ok(())
    }
}

#[derive(Debug, clap::Parser)]
pub struct Link {
    /// The human-readable ID of the child document
    child: Hrid,

    /// The human-readable ID of the parent document
    parent: Hrid,
}

impl Link {
    #[instrument]
    fn run(self, root: PathBuf) -> anyhow::Result<()> {
        let directory = Directory::new(root);
        let msg = format!("Linked {} to {}", self.child, self.parent);

        directory.link_requirement(self.child, self.parent)?;

        println!("{msg}");

        Ok(())
    }
}

#[derive(Debug, clap::Parser)]
pub struct Clean {}

impl Clean {
    #[instrument]
    fn run(path: PathBuf) -> anyhow::Result<()> {
        Directory::new(path).load_all()?.update_hrids()?;
        Ok(())
    }
}
