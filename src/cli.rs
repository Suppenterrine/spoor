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
use crate::sources::{load_sources_or_embedded, SourceSpec};
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
    #[command(after_help = "EXAMPLES:\n  Find a word for 'sky thunder king' with etymology:\n    spoor find \"sky thunder king\" --explain\n\n  Find 3 German words for 'Werkzeug für Wald und Baum':\n    spoor find \"Werkzeug fuer Wald und Baum\" --count 3 --explain\n\n  Find a word with semantic expansion (Datamuse API):\n    spoor find \"synchronize logs distributed\" --online --count 3")]
    Find {
        /// Use-case description (one quoted string)
        query: String,

        /// Number of results (default: [output] count from config, else 1)
        #[arg(long)]
        count: Option<usize>,

        /// Comma-separated system filters
        #[arg(long)]
        systems: Option<String>,

        /// Include detailed explanations
        #[arg(long)]
        explain: bool,

        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,

        /// Force online expansion (fails if unavailable). Default: automatic —
        /// online when the Datamuse endpoint is reachable, local otherwise.
        #[arg(long)]
        online: bool,

        /// Local search only, no network access
        #[arg(long, conflicts_with = "online")]
        offline: bool,

        /// Also return words identical to a query word (default: filtered out)
        #[arg(long)]
        allow_echo: bool,

        /// Only words with this register, e.g. poetic, figurative, literary, archaic
        #[arg(long)]
        register: Option<String>,
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
    /// Latin-script display form (equals `word` for Latin-script entries)
    display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    translit: Option<String>,
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
    /// Result source: "online (datamuse)" or "lokal (<grund>)"
    mode: String,
    matches: Vec<FindMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    candidates: Vec<String>,
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
                online,
                offline,
                allow_echo,
                register,
            } => {
                let (cfg, db, _bootstrapped) = open_context_bootstrapped(self.config.as_deref())?;
                let script = cfg.output.script;
                let count = count.unwrap_or(cfg.output.count.max(1));

                // Get records filtered by systems in SQL
                let systems_filter = systems.map(|s| split_comma_list(&s));
                let mut records = db.all_records(systems_filter.as_deref())?;

                // --register: keep only words carrying that register marker
                if let Some(ref reg) = register {
                    let reg = reg.to_lowercase();
                    records.retain(|r| {
                        r.registers
                            .as_deref()
                            .map_or(false, |regs| regs.contains(reg.as_str()))
                    });
                }

                // The concept bridge must see the whole vocabulary: a German
                // query token finds its English gloss over the German record
                // even when --systems narrows the ranked candidates.
                let bridge_records = if systems_filter.is_some() {
                    db.all_records(None)?
                } else {
                    Vec::new()
                };
                let bridge: &[crate::db::WordRecord] = if systems_filter.is_some() {
                    &bridge_records
                } else {
                    &records
                };

                // Determine candidates for ranking. Online expansion is the
                // DEFAULT: a fast connectivity probe decides; without net
                // (or without config) we fall back to local, quietly but
                // visibly labeled. --offline skips the network entirely,
                // --online forces it and fails loudly when unavailable.
                let expansion_spec = if offline {
                    None
                } else {
                    load_sources_or_embedded("sources.yaml")
                        .ok()
                        .and_then(|s| s.query_expansion)
                };

                let (candidates, mode) = match expansion_spec {
                    None if online => {
                        return Err(anyhow::anyhow!(
                            "--online braucht einen query_expansion-Block in sources.yaml\n\nHinweis: Siehe sources.yaml in der Dokumentation"
                        ));
                    }
                    None if offline => (Vec::new(), "lokal (--offline)".to_string()),
                    None => (Vec::new(), "lokal (keine query_expansion konfiguriert)".to_string()),
                    Some(spec) => {
                        if !online && !crate::fetch::quick_connectivity_check(&spec.url) {
                            (Vec::new(), "lokal (kein Netz)".to_string())
                        } else {
                            match crate::fetch::expand_query(&spec, &query) {
                                Ok(cands) => (cands, "online (datamuse)".to_string()),
                                Err(e) if online => {
                                    eprintln!("Fehler bei der semantischen Expansion: {}", e);
                                    eprintln!("Hinweis: Bitte erneut versuchen oder --offline verwenden");
                                    std::process::exit(1);
                                }
                                Err(_) => (Vec::new(), "lokal (Netzfehler)".to_string()),
                            }
                        }
                    }
                };
                let is_online = mode.starts_with("online");

                // Nexus edges for the association hop: only rows whose source
                // is a query token or one of its bridge concepts
                let edge_srcs = lookup::edge_source_terms(&query, bridge);
                let edges = db.edges_from(&edge_srcs)?;

                // Rank: concept bridge + association hop + anti-echo + origin/register bonus
                let matches =
                    lookup::rank_semantic(&records, bridge, &query, &candidates, &edges, allow_echo);

                if matches.is_empty() {
                    let hint = if systems_filter.is_some() {
                        "es ohne --systems zu versuchen, um mehr Treffer zu finden"
                    } else {
                        "spoor db fetch --limit 5000 auszufuehren, um mehr Woerter zu laden"
                    };
                    eprintln!("Keine Treffer fuer '{}'.", query);
                    eprintln!("Naechster Schritt: {} oder mit anderen Schluesseln suchen", hint);

                    // Add suggestions line if available
                    let suggestions = lookup::suggest(&records, &query, 5);
                    if !suggestions.is_empty() {
                        eprintln!("Aehnliche Woerter im Bestand: {}", suggestions.join(", "));
                    }

                    // If the only hit would have been the query word itself, say so
                    if !allow_echo {
                        let tokens = lookup::tokenize(&query);
                        let echo_suppressed = records
                            .iter()
                            .any(|r| tokens.iter().any(|t| r.word.to_lowercase() == *t));
                        if echo_suppressed {
                            eprintln!("Hinweis: Das Anfrage-Wort selbst steht im Bestand, wird aber als Echo gefiltert (--allow-echo zeigt es).");
                        }
                    }

                    // If online and candidates were found but no matches
                    if is_online && !candidates.is_empty() {
                        eprintln!("Semantische Kandidaten (Datamuse) ohne Eintrag im Bestand: {}", candidates.join(", "));
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
                            display: display_word(&m.record, script),
                            translit: m.record.translit.clone(),
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
                        mode,
                        matches: json_matches,
                        candidates: if is_online { candidates } else { Vec::new() },
                    };
                    println!("{}", serde_json::to_string_pretty(&out)?);
                } else {
                    // Text format: check if stdout is a TTY
                    let is_tty = console::user_attended();

                    // State the result source (online vs. local fallback):
                    // dim header on TTY, stderr note otherwise (stdout stays script-safe)
                    if is_tty {
                        println!("{}", style(format!("Quelle {}", mode)).dim());
                    } else {
                        eprintln!("Quelle: {}", mode);
                    }

                    for m in matches.iter().take(take_count) {
                        if is_tty {
                            // TTY: compact rich block; --explain adds the root line
                            print!("{}", format_match_rich(m, script, explain));
                        } else if explain {
                            // Non-TTY --explain: single justification line per word
                            println!("{}", lookup::explain_display(m, &display_word(&m.record, script)));
                        } else {
                            // Non-TTY: plain one-word-per-line (script-safe)
                            println!("{}", display_word(&m.record, script));
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
                        println!("Total edges: {}", db.edge_count()?);
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
                        let mut specs: Vec<SourceSpec> = load_sources_or_embedded(&file)
                            .with_context(|| {
                                format!(
                                    "Quellendatei konnte nicht geladen werden: {}\n\nHinweis: Sie koennen auch ohne externe Quellen arbeiten und mehr Woerter mit 'spoor db import <csv>' hinzufuegen.",
                                    file
                                )
                            })?
                            .sources;

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
                            "Imported {} words and {} edges from {} sources.",
                            outcome.total_inserted,
                            outcome.total_edges,
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

/// Latin display form of a record's word (stored romanization first,
/// rule-based fallback second, the word itself if already Latin).
fn latin_of(record: &crate::db::WordRecord) -> String {
    crate::translit::latin_form(
        &record.word,
        record.translit.as_deref(),
        record.language.as_deref(),
    )
}

/// The word as shown to the user, controlled by `[output] script`.
fn display_word(record: &crate::db::WordRecord, script: crate::config::Script) -> String {
    use crate::config::Script;
    let latin = latin_of(record);
    match script {
        Script::Native => record.word.clone(),
        Script::Latin => latin,
        Script::Both => {
            if latin != record.word {
                format!("{} ({})", latin, record.word)
            } else {
                record.word.clone()
            }
        }
    }
}

/// Truncate to a maximum number of chars, appending an ellipsis if cut.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// The record's meaning for the head line: gloss phrases joined by "; ",
/// packed until `max` chars are full.
fn meaning_of(record: &crate::db::WordRecord, max: usize) -> Option<String> {
    let tags = record.tags.as_deref()?;
    let mut out = String::new();
    for phrase in tags.split(',') {
        let p = phrase.trim();
        if p.is_empty() {
            continue;
        }
        if out.is_empty() {
            out.push_str(p);
        } else if out.chars().count() + p.chars().count() + 2 <= max {
            out.push_str("; ");
            out.push_str(p);
        } else {
            break;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(truncate_chars(&out, max))
    }
}

/// Deduplicate matched entries and cap them for one compact line ("+n" tail).
fn spur_compact(matched: &[String], max: usize) -> String {
    let mut seen = HashSet::new();
    let mut out: Vec<&str> = Vec::new();
    for m in matched {
        if seen.insert(m.as_str()) {
            out.push(m);
        }
    }
    let extra = out.len().saturating_sub(max);
    out.truncate(max);
    let mut s = out.join(" · ");
    if extra > 0 {
        s.push_str(&format!(" · +{}", extra));
    }
    s
}

/// Format a match as a compact rich block (for TTY output).
///
///   word (native) — meaning  [lang · system]     word bold cyan, rest quiet
///     Spur    baum → tree (bruecke) · +1          dim green, deduplicated
///     Wurzel  from ancient greek ... (grc)        only with --explain
fn format_match_rich(m: &lookup::Match, script: crate::config::Script, show_root: bool) -> String {
    let record = &m.record;
    let shown = display_word(record, script);
    let latin = latin_of(record);

    let mut head = style(&shown).bold().cyan().to_string();
    if script == crate::config::Script::Latin && latin != record.word {
        head.push_str(&format!(" {}", style(format!("({})", record.word)).dim()));
    }
    if let Some(meaning) = meaning_of(record, 64) {
        head.push_str(&format!(" — {}", meaning));
    }
    let lang = record.language.as_deref().unwrap_or("?");
    let system = record.system.as_deref().unwrap_or("?");
    let registers = record
        .registers
        .as_deref()
        .map(|r| format!(" · {}", r.replace(',', " · ")))
        .unwrap_or_default();
    head.push_str(&format!(
        "  {}",
        style(format!("[{} · {}{}]", lang, system, registers)).dim()
    ));

    // Row labels via structure, not color: dot-padded to equal width with a
    // trailing colon ("Spur..:" / "Wurzel:") so they read as line titles.
    let mut lines = vec![head];
    lines.push(format!(
        "  {} {}",
        style("Spur..:").dim(),
        style(spur_compact(&m.matched, 4)).green().dim()
    ));

    if show_root {
        if let Some(e) = record.etymology.as_deref() {
            let origin = record
                .origin_lang
                .as_deref()
                .map(|o| format!(" ({})", o))
                .unwrap_or_default();
            lines.push(format!(
                "  {} {}",
                style("Wurzel:").dim(),
                style(truncate_chars(&format!("{}{}", e, origin), 110)).dim()
            ));
        }
    }

    lines.join("\n") + "\n"
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

