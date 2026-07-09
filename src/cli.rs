use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Serialize;

use crate::config::Config;
use crate::db::Db;
use crate::fetch::{fetch_all, FetchProgress, FetchReport};
use crate::generator::{Generator, SeededRng, WordLists};
use crate::lookup;
use crate::sources::{load_sources, SourceSpec};
use crate::SEED_WORDS_CSV;

#[derive(Parser, Debug)]
#[command(name = "spoor")]
#[command(version)]
#[command(about = "spoor — findet Namen ueber Bedeutung (find) und generiert sie reproduzierbar (gen)")]
#[command(propagate_version = true)]
#[command(after_help = "ARBEITSMODELL:\n  spoor find <beschreibung>   Reverse-Lookup: Bedeutung -> ein passendes Wort mit Herkunft\n  spoor gen --seed <n>         Reproduzierbare Namen-Generierung nach Seed\n  spoor db fetch               Optionale Erweiterung des Wortbestands aus Online-Quellen\n\nWeitere Kommandos und Optionen: spoor help")]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to configuration file (optional; defaults to config.toml or system data dir)
    #[arg(long, global = true)]
    config: Option<String>,
}

#[derive(Subcommand, Debug, Clone)]
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
    #[command(after_help = "EXAMPLES:\n  Find a word for 'sky thunder king' with etymology:\n    spoor find \"sky thunder king\" --explain\n\n  Find 3 German words for 'Werkzeug für Wald und Baum':\n    spoor find \"Werkzeug fuer Wald und Baum\" --count 3 --explain")]
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

#[derive(Subcommand, Debug, Clone)]
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

#[derive(Subcommand, Debug, Clone)]
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
        match self.command.clone() {
            None => {
                // Bare invocation: show status screen
                let (cfg, db, _bootstrapped) = open_context_bootstrapped(self.config.as_deref())?;
                print_status_screen(&db, &cfg.db.path)?;
                Ok(())
            }
            Some(cmd) => self.handle_command(cmd),
        }
    }

    fn handle_command(&self, command: Commands) -> anyhow::Result<()> {
        match command {
            Commands::Gen {
                seed,
                count,
                systems,
                template,
                format,
            } => {
                let (cfg, db, _bootstrapped) = open_context_bootstrapped(self.config.as_deref())?;
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
                let (_cfg, db, _bootstrapped) = open_context_bootstrapped(self.config.as_deref())?;

                // Get records filtered by systems in SQL
                let systems_filter = systems.map(|s| split_comma_list(&s));
                let records = db.all_records(systems_filter.as_deref())?;

                // Rank records against the query
                let matches = lookup::rank(&records, &query);

                if matches.is_empty() {
                    let hint = if systems_filter.is_some() {
                        "es ohne --systems zu versuchen, um mehr Treffer zu finden"
                    } else {
                        "spoor db fetch --limit 1000 auszufuehren, um mehr Woerter zu laden"
                    };
                    eprintln!("Keine Treffer fuer '{}'.", query);
                    eprintln!("Naechster Schritt: {} oder mit anderen Schluesseln suchen", hint);

                    // Add suggestions line if available
                    let suggestions = lookup::suggest(&records, &query, 5);
                    if !suggestions.is_empty() {
                        eprintln!("Aehnliche Woerter im Bestand: {}", suggestions.join(", "));
                    }

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
                    // Text format: check if stdout is a TTY
                    let is_tty = console::user_attended();

                    for m in matches.iter().take(take_count) {
                        if is_tty && !explain {
                            // TTY without --explain: use rich block format (new default)
                            print!("{}", format_match_rich(m));
                        } else if explain {
                            // --explain: always use the single-line explain format
                            println!("{}", lookup::explain(m));
                        } else {
                            // Non-TTY: plain one-word-per-line
                            println!("{}", m.record.word);
                        }
                    }
                }
            }
            Commands::List(list_cmd) => {
                let (_cfg, db, _bootstrapped) = open_context_bootstrapped(self.config.as_deref())?;
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
                        let (_cfg, mut db, _bootstrapped) = open_context_bootstrapped(self.config.as_deref())?;
                        if !path.exists() {
                            return Err(anyhow::anyhow!(
                                "Datei nicht gefunden: {}\n\nBitte ueberpruefen Sie den Pfad und versuchen Sie es erneut.",
                                path.display()
                            ));
                        }
                        let report = db.import_csv(&path)?;
                        println!("Imported {} words.", report.imported);
                        if report.unknown_class > 0 {
                            println!("Warning: {} words have an unrecognized word_class and will be ignored by 'gen'.", report.unknown_class);
                        }
                    }
                    DbCommand::Info => {
                        let (_cfg, db, _bootstrapped) = open_context_bootstrapped(self.config.as_deref())?;
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
                        let (_cfg, mut db, _bootstrapped) = open_context_bootstrapped(self.config.as_deref())?;
                        let mut specs: Vec<SourceSpec> = match load_sources(&file) {
                            Ok(sources) => sources.sources,
                            Err(_) => {
                                return Err(anyhow::anyhow!(
                                    "Quellendatei nicht gefunden: {}\n\nsources.yaml sollte sich im Repository-Verzeichnis befinden.\n\nHinweis: Sie koennen auch ohne externe Quellen arbeiten und mehr Woerter mit 'spoor db import <csv>' hinzufuegen.",
                                    file
                                ));
                            }
                        };

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

/// Print a German status screen showing database stats and usage hints
fn print_status_screen(db: &Db, db_path: &std::path::Path) -> anyhow::Result<()> {
    let stats = db.stats()?;
    let version = env!("CARGO_PKG_VERSION");

    // Format language summary: top 4 languages with counts
    let mut lang_summary = String::new();
    for (i, (lang, count)) in stats.by_language.iter().take(4).enumerate() {
        if i > 0 {
            lang_summary.push_str(" · ");
        }
        lang_summary.push_str(&format!("{} {}", lang, count));
    }
    if stats.by_language.len() > 4 {
        lang_summary.push_str(" · ...");
    }

    println!("spoor {} — folge der Bedeutung zum Namen\n", version);
    println!("  Wortbestand: {} Woerter ({})", stats.total, lang_summary);
    println!("  Datenbank:   {}\n", db_path.display());
    println!("WOMIT MOECHTEST DU STARTEN?\n");
    println!("  Einen Namen zum Anwendungsfall finden:");
    println!("    spoor find \"werkzeug fuer wald und baum\" --explain\n");
    println!("  Zufaellige Namen generieren (reproduzierbar):");
    println!("    spoor gen --seed 42 --count 5\n");
    println!("  Mehr Woerter laden (kaikki.org, konfiguriert in sources.yaml):");
    println!("    spoor db fetch --limit 1000\n");
    println!("Alle Kommandos: spoor help");

    Ok(())
}

/// Open database with auto-bootstrap logic. Returns (Config, Db, was_bootstrapped).
/// If the DB file doesn't exist, seeds it with embedded CSV and prints init message to stderr.
fn open_context_bootstrapped(config: Option<&str>) -> anyhow::Result<(Config, Db, bool)> {
    let (path, explicit) = match config {
        Some(p) => (p, true),
        None => ("config.toml", false),
    };

    let cfg = Config::load(path, explicit)?;
    let db_path = &cfg.db.path;

    // Check if DB file exists before opening
    let db_exists = db_path.exists();

    // Create parent directory if needed
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directory for {}", db_path.display()))?;
    }

    let mut db = Db::open(db_path)?;

    if !db_exists {
        // Bootstrap: import seed data
        let report = db.import_csv_reader(SEED_WORDS_CSV.as_bytes())?;
        eprintln!(
            "Initialized word database with {} curated words at {}",
            report.imported,
            db_path.display()
        );
        Ok((cfg, db, true))
    } else {
        Ok((cfg, db, false))
    }
}

/// Legacy function for backward compatibility (no bootstrap)
#[allow(dead_code)]
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

/// Format a match as a rich block (for TTY output).
/// Returns a multi-line formatted string with color styling.
/// Structure:
///   word (bold, cyan)
///   system · etymology (origin_lang) (dim; system magenta)
///   Treffer: matched tokens (dim green)
fn format_match_rich(m: &lookup::Match) -> String {
    let word = style(&m.record.word).bold().cyan().to_string();

    let etymology = m.record.etymology.as_deref().unwrap_or("?");
    let origin_lang = m.record.origin_lang.as_deref().unwrap_or("?");
    let system = m.record.system.as_deref().unwrap_or("?");

    // Second line: dim system (in magenta) and etymology with origin lang
    let system_line = format!(
        "  {} · {} ({})",
        style(system).magenta().dim(),
        style(etymology).dim(),
        origin_lang
    );

    // Third line: dim green tags with matched hits
    let matched = m.matched.join(" · ");
    let tags_str = m.record.tags.as_deref().unwrap_or("");
    let tags_suffix = if !tags_str.is_empty() {
        format!(" · {}", tags_str)
    } else {
        String::new()
    };

    let treffer_line = format!(
        "  {}{}",
        style(format!("Treffer: {}", matched)).green().dim(),
        tags_suffix
    );

    format!("{}\n{}\n{}\n", word, system_line, treffer_line)
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

