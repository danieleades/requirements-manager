//! Plain-text Requirements Management
//!
//! Requirements are markdown documents stored in a directory.

use clap::Parser;

mod cli;

fn main() {
    let cli = cli::Cli::parse();
    cli.run();
}
