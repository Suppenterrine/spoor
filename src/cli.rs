use std::collections::HashSet;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use serde::Serialize;

use crate::config::Config;
use crate::db::Db;
use crate::generator::{Generator, SeededRng, WordLists};

#[derive(Parser, Debug)]
#[command(name = "name-generator")]
#[command(about = "Generates themed names from a local word database", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate one or more names
    Gen {
        /// Seed for deterministic generation
        #[arg(long)]
        seed: Option<u64>,

        /// Number of results
        #[arg(long, default_value_t = 1)]
        count: usize,

        /// Comma-separated system filters
        #[arg(long)]
        systems: Option<String>,

        /// Optional custom template
        #[arg(long)]
        template: Option<String>,

        /// Output format
        #[arg(long, default_value = "text")]
        format: String,

        /// Alternate config path
        #[arg(long, default_value = "config.toml")]
        config: String,
    },
    /// Import CSV into the database
    Import {
        /// CSV file path
        path: PathBuf,

        /// Alternate config path
        #[arg(long, default_value = "config.toml")]
        config: String,
    },
    /// Database statistics
    Info {
        /// Alternate config path
        #[arg(long, default_value = "config.toml")]
        config: String,
    },
}

#[derive(Serialize)]
struct GenOutput {
    seed: u64,
    names: Vec<String>,
}

impl Cli {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            Commands::Gen {
                seed,
                count,
                systems,
                template,
                format,
                config,
            } => {
                let (cfg, db) = open_context(&config)?;
                let words = load_wordlists(&db, systems.as_deref())?;

                let generator = match template {
                    Some(t) => Generator::with_template(&cfg, words, &t)?,
                    None => Generator::new(&cfg, words),
                };
                let mut names = Vec::with_capacity(count);
                let mut used = HashSet::new();

                let seed_was_given = seed.is_some();
                let seed = seed.unwrap_or_else(rand::random::<u64>);
                let mut srng = SeededRng::new(seed);

                for _ in 0..count {
                    match generator.generate_unique(&mut srng, &mut used, 100) {
                        Some(name) => names.push(name),
                        None => {
                            if names.is_empty() {
                                return Err(anyhow::anyhow!("no words available - import data first (name-generator import data/words.csv)"));
                            } else {
                                eprintln!("Warning: only {} unique names were possible; stopping early", names.len());
                                break;
                            }
                        }
                    }
                }

                if format == "json" {
                    let out = GenOutput {
                        seed,
                        names,
                    };
                    println!("{}", serde_json::to_string_pretty(&out)?);
                } else {
                    if !seed_was_given {
                        println!("seed={}", seed);
                    }
                    for name in names {
                        println!("{}", name);
                    }
                }
            }
            Commands::Import { path, config } => {
                let (_cfg, mut db) = open_context(&config)?;
                let records = csv_import::read_words(&path)?;
                db.insert_words(&records)?;
                println!("Imported {} words.", records.len());
            }
            Commands::Info { config } => {
                let (_cfg, db) = open_context(&config)?;
                let stats = db.stats()?;
                let total = stats.get("total").copied().unwrap_or(0);
                println!("Total words: {total}");
                for (k, v) in &stats {
                    if k == "total" {
                        continue;
                    }
                    println!("{}: {}", k, v);
                }
            }
        }
        Ok(())
    }
}

fn open_context(config_path: &str) -> anyhow::Result<(Config, Db)> {
    let cfg = Config::read(Path::new(config_path))?;
    let db = Db::open(PathBuf::from(&cfg.db.path))?;
    Ok((cfg, db))
}

fn load_wordlists(db: &Db, systems: Option<&str>) -> anyhow::Result<WordLists> {
    let mut prefixes = Vec::new();
    let mut words = Vec::new();
    let mut suffix_adjs = Vec::new();
    let mut suffix_names = Vec::new();

    let systems_filter = systems.map(|s| {
        s.split(',')
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>()
    });

    let sql = "SELECT system, word, word_class FROM words";
    let mut stmt = db.conn().prepare(sql)?;
    let rows = stmt.query_map([], |row| {
        let system: String = row.get(0)?;
        let word: String = row.get(1)?;
        let word_class: String = row.get(2)?;
        Ok((system, word, word_class))
    })?;

    for row in rows {
        let (system, word, word_class) = row?;
        if let Some(ref filters) = systems_filter {
            if !filters.contains(&system) {
                continue;
            }
        }

        match word_class.as_str() {
            "prefix" => prefixes.push(word),
            "noun" | "proper" => words.push(word),
            "adj" => suffix_adjs.push(word),
            "suffix_noun" | "suffix" => suffix_names.push(word),
            _ => {}
        }
    }

    Ok(WordLists {
        prefixes,
        words,
        suffix_adjs,
        suffix_names,
    })
}

mod csv_import {
    use std::path::Path;
    use crate::db::WordRecord;

    pub fn read_words(path: impl AsRef<Path>) -> anyhow::Result<Vec<WordRecord>> {
        let mut reader = csv::Reader::from_path(path)?;
        let mut records = Vec::new();
        for result in reader.records() {
            let record = result?;
            let rec = WordRecord::parse_csv_record(&record)?;
            records.push(rec);
        }
        Ok(records)
    }
}
