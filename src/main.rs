use clap::Parser;
mod cli;
mod config;
mod db;
mod generator;

use crate::cli::Cli;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    cli.run()
}
