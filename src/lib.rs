pub mod cli;
pub mod config;
pub mod db;
pub mod fetch;
pub mod generator;
pub mod lookup;
pub mod sources;

pub use config::Config;
pub use db::{Db, WordRecord};
pub use generator::{Generator, SeededRng, WordLists};
pub use lookup::{Match as LookupMatch};

pub use anyhow::Result;
pub use anyhow::anyhow;

/// Embedded seed data (77 curated words)
pub const SEED_WORDS_CSV: &str = include_str!("../data/words.csv");
