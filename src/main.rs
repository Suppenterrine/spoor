use clap::Parser;
mod cli;
mod config;
mod db;
mod generator;
mod lookup;

use crate::cli::Cli;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    cli.run()
}
