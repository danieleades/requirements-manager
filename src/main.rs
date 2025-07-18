//! Plain-text Requirements Management
//!
//! Requirements are markdown documents stored in a directory.

mod domain;
use clap::Parser;
pub use domain::Requirement;

mod storage;

mod cli;

fn main() {
    let cli = cli::Cli::parse();
    cli.run();
}
