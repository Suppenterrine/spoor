use std::collections::HashSet;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use serde::Serialize;

use crate::config::Config;
use crate::db::Db;
use crate::generator::{Generator, SeededRng, WordLists};

#[derive(Parser, Debug)]
#[command(name = "name-generator")]
#[command(version)]
#[command(about = "Generates themed names from a local word database")]
#[command(arg_required_else_help = true)]
#[command(subcommand_required = true)]
#[command(propagate_version = true)]
#[command(after_help = "For more information, see docs/reference/cli.md")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to configuration file
    #[arg(long, global = true, default_value = "config.toml")]
    config: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate one or more names
    #[command(after_help = "EXAMPLES:\n  Generate 1 name with a random seed:\n    name-generator gen\n\n  Generate 3 names with seed 42:\n    name-generator gen --seed 42 --count 3\n\n  Generate names only from the 'nature' system:\n    name-generator gen --systems nature --count 5")]
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

        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Explore the word database
    #[command(subcommand)]
    List(ListCommand),
    /// Database maintenance
    #[command(subcommand)]
    Db(DbCommand),
}

#[derive(Subcommand, Debug)]
enum ListCommand {
    /// List all systems with word counts
    Systems,
    /// List all languages with word counts
    Languages,
    /// List all word classes with word counts
    Classes,
    /// List words (optionally filtered by system and/or language)
    Words {
        /// Filter by system
        #[arg(long)]
        system: Option<String>,

        /// Filter by language
        #[arg(long)]
        language: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum DbCommand {
    /// Import a CSV into the database
    Import {
        /// CSV file path
        path: PathBuf,
    },
    /// Database statistics
    Info,
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
            } => {
                let (cfg, db) = open_context(&self.config)?;
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
                                return Err(anyhow::anyhow!("no words available - import data first (name-generator db import data/words.csv)"));
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
            Commands::List(list_cmd) => {
                let (_cfg, db) = open_context(&self.config)?;
                match list_cmd {
                    ListCommand::Systems => {
                        let systems = db.list_systems()?;
                        for (name, count) in systems {
                            println!("{:<20} {}", name, count);
                        }
                    }
                    ListCommand::Languages => {
                        let languages = db.list_languages()?;
                        for (name, count) in languages {
                            println!("{:<20} {}", name, count);
                        }
                    }
                    ListCommand::Classes => {
                        let classes = db.list_classes()?;
                        for (name, count) in classes {
                            println!("{:<20} {}", name, count);
                        }
                    }
                    ListCommand::Words { system, language } => {
                        let words = db.list_words(system.as_deref(), language.as_deref())?;
                        for (word, lang, sys, class) in words {
                            println!(
                                "{:<20} {} / {} / {}",
                                word,
                                if lang.is_empty() { "?" } else { &lang },
                                if sys.is_empty() { "?" } else { &sys },
                                if class.is_empty() { "?" } else { &class }
                            );
                        }
                    }
                }
            }
            Commands::Db(db_cmd) => {
                match db_cmd {
                    DbCommand::Import { path } => {
                        let (_cfg, mut db) = open_context(&self.config)?;
                        let records = csv_import::read_words(&path)?;
                        db.insert_words(&records)?;
                        println!("Imported {} words.", records.len());
                    }
                    DbCommand::Info => {
                        let (_cfg, db) = open_context(&self.config)?;
                        let stats = db.stats()?;
                        println!("Total words: {}", stats.total);
                        println!("\nBy language:");
                        for (lang, count) in &stats.by_language {
                            println!("  {}: {}", lang, count);
                        }
                        println!("\nBy system:");
                        for (sys, count) in &stats.by_system {
                            println!("  {}: {}", sys, count);
                        }
                    }
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

    let word_class_rows = db.words_by_class(systems_filter.as_deref())?;

    for (word, word_class) in word_class_rows {
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
