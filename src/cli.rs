use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Parser, Subcommand};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Serialize;

use crate::config::Config;
use crate::db::Db;
use crate::fetch::{fetch_all, FetchProgress, FetchReport};
use crate::generator::{Generator, SeededRng, WordLists};
use crate::lookup;
use crate::sources::{load_sources, SourceSpec};

#[derive(Parser, Debug)]
#[command(name = "spoor")]
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
    #[command(after_help = "EXAMPLES:\n  Generate 1 name with a random seed:\n    spoor gen\n\n  Generate 3 names with seed 42:\n    spoor gen --seed 42 --count 3\n\n  Generate names only from the 'nature' system:\n    spoor gen --systems nature --count 5")]
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
    /// Find a single fitting word for a use-case description
    #[command(after_help = "EXAMPLES:\n  Find a word for 'sky thunder king':\n    spoor find \"sky thunder king\"\n\n  Find 3 German words with explanations:\n    spoor find \"Werkzeug für Wald und Baum\" --count 3 --explain")]
    Find {
        /// Use-case description (one quoted string)
        query: String,

        /// Number of results
        #[arg(long, default_value_t = 1)]
        count: usize,

        /// Comma-separated system filters
        #[arg(long)]
        systems: Option<String>,

        /// Include detailed explanations
        #[arg(long)]
        explain: bool,

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
    /// Download and import word sources over the network (see sources.yaml)
    #[command(after_help = "EXAMPLES:\n  Fetch all configured sources:\n    spoor db fetch\n\n  Fetch only one source, capped at 50 words:\n    spoor db fetch --only kaikki-la --limit 50")]
    Fetch {
        /// Path to the sources.yaml file
        #[arg(long, default_value = "sources.yaml")]
        file: String,

        /// Comma-separated list of source ids to fetch (default: all)
        #[arg(long)]
        only: Option<String>,

        /// Override max_words for every selected source
        #[arg(long)]
        limit: Option<usize>,
    },
}

#[derive(Serialize)]
struct GenOutput {
    seed: u64,
    names: Vec<String>,
}

#[derive(Serialize)]
struct FindMatch {
    word: String,
    score: f64,
    etymology: Option<String>,
    origin_lang: Option<String>,
    system: Option<String>,
    tags: Option<String>,
    matched: Vec<String>,
}

#[derive(Serialize)]
struct FindOutput {
    query: String,
    matches: Vec<FindMatch>,
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
                                return Err(anyhow::anyhow!("no words available - import data first (spoor db import data/words.csv)"));
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
            Commands::Find {
                query,
                count,
                systems,
                explain,
                format,
            } => {
                let (_cfg, db) = open_context(&self.config)?;

                // Get records filtered by systems in SQL
                let systems_filter = systems.map(|s| split_comma_list(&s));
                let records = db.all_records(systems_filter.as_deref())?;

                // Rank records against the query
                let matches = lookup::rank(&records, &query);

                if matches.is_empty() {
                    eprintln!("no matches - try other keywords or import more data");
                    std::process::exit(1);
                }

                let take_count = std::cmp::min(count, matches.len());

                if format == "json" {
                    let json_matches: Vec<FindMatch> = matches
                        .iter()
                        .take(take_count)
                        .map(|m| FindMatch {
                            word: m.record.word.clone(),
                            score: m.score,
                            etymology: m.record.etymology.clone(),
                            origin_lang: m.record.origin_lang.clone(),
                            system: m.record.system.clone(),
                            tags: m.record.tags.clone(),
                            matched: m.matched.clone(),
                        })
                        .collect();

                    let out = FindOutput {
                        query,
                        matches: json_matches,
                    };
                    println!("{}", serde_json::to_string_pretty(&out)?);
                } else {
                    // Text format
                    for m in matches.iter().take(take_count) {
                        if explain {
                            println!("{}", lookup::explain(m));
                        } else {
                            println!("{}", m.record.word);
                        }
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
                        let report = db.import_csv(&path)?;
                        println!("Imported {} words.", report.imported);
                        if report.unknown_class > 0 {
                            println!("Warning: {} words have an unrecognized word_class and will be ignored by 'gen'.", report.unknown_class);
                        }
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
                    DbCommand::Fetch { file, only, limit } => {
                        let (_cfg, mut db) = open_context(&self.config)?;
                        let mut specs: Vec<SourceSpec> = load_sources(&file)?.sources;

                        if let Some(only) = only {
                            let wanted: HashSet<String> = split_comma_list(&only).into_iter().collect();
                            let known: HashSet<&str> = specs.iter().map(|s| s.id.as_str()).collect();
                            for id in &wanted {
                                if !known.contains(id.as_str()) {
                                    eprintln!("Warning: unknown source id '{}' (ignored)", id);
                                }
                            }
                            specs.retain(|s| wanted.contains(&s.id));
                        }

                        if let Some(limit) = limit {
                            for spec in specs.iter_mut() {
                                spec.max_words = limit;
                            }
                        }

                        if specs.is_empty() {
                            println!("No sources selected.");
                            return Ok(());
                        }

                        println!("[+] Fetching {} sources", specs.len());

                        let multi = MultiProgress::new();
                        let progress = CliFetchProgress::new(&multi, &specs);
                        let outcome = fetch_all(&mut db, &specs, &progress)?;

                        println!(
                            "Imported {} words from {} sources.",
                            outcome.total_inserted,
                            outcome.reports.len()
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

/// docker-compose-style progress UI for `db fetch`: one self-updating
/// spinner line per source inside a shared `MultiProgress`. UI-only; never
/// touches the `Db` (workers call this directly from their own threads).
struct CliFetchProgress {
    bars: HashMap<String, ProgressBar>,
}

impl CliFetchProgress {
    fn new(multi: &MultiProgress, specs: &[SourceSpec]) -> Self {
        let mut bars = HashMap::new();
        for spec in specs {
            let bar = multi.add(ProgressBar::new_spinner());
            bar.set_style(Self::spinner_style());
            bar.enable_steady_tick(Duration::from_millis(120));
            bar.set_prefix(spec.id.clone());
            bar.set_message("warte auf Antwort...");
            bars.insert(spec.id.clone(), bar);
        }
        Self { bars }
    }

    fn spinner_style() -> ProgressStyle {
        ProgressStyle::with_template("{spinner} {prefix:<14} {msg}")
            .expect("valid progress template")
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏⠿⠿")
    }

    fn finished_style(symbol: &str) -> ProgressStyle {
        ProgressStyle::with_template(&format!("{symbol} {{prefix:<14}} {{msg}}"))
            .expect("valid progress template")
    }

    fn format_mb(bytes: u64) -> String {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    }
}

impl FetchProgress for CliFetchProgress {
    fn on_update(&self, id: &str, bytes: u64, accepted: usize, skipped: usize) {
        if let Some(bar) = self.bars.get(id) {
            bar.set_message(format!(
                "{} · {} Woerter · {} uebersprungen",
                Self::format_mb(bytes),
                accepted,
                skipped
            ));
        }
    }

    fn on_done(&self, id: &str, report: &FetchReport) {
        if let Some(bar) = self.bars.get(id) {
            bar.set_style(Self::finished_style("\u{2714}")); // ✔
            bar.finish_with_message(format!(
                "{} Woerter importiert ({} gelesen)",
                report.accepted,
                Self::format_mb(report.bytes_read)
            ));
        }
    }

    fn on_error(&self, id: &str, msg: &str) {
        if let Some(bar) = self.bars.get(id) {
            bar.set_style(Self::finished_style("\u{2716}")); // ✖
            bar.finish_with_message(msg.to_string());
        }
    }
}

fn open_context(config_path: &str) -> anyhow::Result<(Config, Db)> {
    let cfg = Config::read(Path::new(config_path))?;
    let db = Db::open(PathBuf::from(&cfg.db.path))?;
    Ok((cfg, db))
}

/// Split a comma-separated CLI value into trimmed parts. Shared by
/// `--systems` (gen/find) and `--only` (db fetch) instead of duplicating the
/// splitting logic per option.
fn split_comma_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|s| s.trim().to_string())
        .collect()
}

fn load_wordlists(db: &Db, systems: Option<&str>) -> anyhow::Result<WordLists> {
    let mut prefixes = Vec::new();
    let mut words = Vec::new();
    let mut suffix_adjs = Vec::new();
    let mut suffix_names = Vec::new();

    let systems_filter = systems.map(split_comma_list);

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

