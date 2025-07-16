mod domain;
use clap::Parser;
pub use domain::Requirement;

mod storage;

mod cli;

fn main() {
    let command = cli::Command::parse();
    command.run();
}
