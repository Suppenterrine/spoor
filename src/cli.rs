use std::collections::HashSet;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
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
    },
    /// Database statistics
    Info {},
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
                template: _,
                format,
                config,
            } => {
                let cfg_path = Path::new(&config);
                let cfg = Config::read(cfg_path)?;

                let db_path = PathBuf::from(&cfg.db.path);
                let db = Db::open(db_path)?;
                let words = load_wordlists(&db, systems.as_deref())?;

                let generator = Generator::new(&cfg, words);
                let mut names = Vec::with_capacity(count);
                let mut used = HashSet::new();

                let mut rng = match seed {
                    Some(s) => ChaCha8Rng::seed_from_u64(s),
                    None => ChaCha8Rng::from_entropy(),
                };
                let used_seed = rng.gen();

                let mut srng = SeededRng::with_rng(rng);

                for _ in 0..count {
                    let name = generator.generate_one(&mut srng, &mut used);
                    names.push(name);
                }

                if format == "json" {
                    let out = GenOutput {
                        seed: used_seed,
                        names,
                    };
                    println!("{}", serde_json::to_string_pretty(&out)?);
                } else {
                    if seed.is_none() {
                        println!("seed={}", used_seed);
                    }
                    for name in names {
                        println!("{}", name);
                    }
                }
            }
            Commands::Import { path } => {
                let cfg = Config::read("config.toml")?;
                let db_path = PathBuf::from(&cfg.db.path);
                let mut db = Db::open(db_path)?;
                let records = csv_import::read_words(&path)?;
                db.insert_words(&records)?;
                println!("Imported {} words.", records.len());
            }
            Commands::Info {} => {
                let cfg = Config::read("config.toml")?;
                let db_path = PathBuf::from(&cfg.db.path);
                let db = Db::open(db_path)?;
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
