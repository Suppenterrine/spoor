use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use flate2::read::GzDecoder;
use serde_json::Value;

use crate::db::{Db, Edge};
use crate::{WordRecord, sources::SourceSpec};

/// Result of parsing one source line/entry: the word plus its association
/// edges for the concept nexus.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLine {
    pub record: WordRecord,
    pub edges: Vec<Edge>,
}

/// Report from processing a single source.
#[derive(Debug, Clone)]
pub struct FetchReport {
    pub id: String,
    pub accepted: usize,
    pub skipped: usize,
    /// Association edges harvested alongside the accepted words.
    pub edges: usize,
    pub bytes_read: u64,
    /// Set if the source failed before/while streaming (e.g. network error).
    pub error: Option<String>,
}

/// Collapse all whitespace runs (incl. newlines) to single spaces. Kaikki
/// etymology texts may contain rendered multi-line "etymology trees"; our
/// explain output and tags are strictly single-line.
fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Sense-level wiktextract tags that mark a variant entry (inflection,
/// spelling variant, abbreviation) instead of a real meaning.
const JUNK_SENSE_TAGS: &[&str] = &[
    "misspelling",
    "abbreviation",
    "initialism",
    "acronym",
    "alt-of",
    "form-of",
];

/// Gloss phrases that mark dictionary bookkeeping (surnames, "... form of X",
/// pure inflections). Checked against the cleaned, lowercased gloss text.
const JUNK_GLOSS_MARKERS: &[&str] = &[
    "surname",
    "given name",
    "misspelling of",
    "abbreviation of",
    "initialism of",
    "acronym of",
    "alternative form of",
    "alternative spelling of",
    "alternative letter-case form of",
    "obsolete form of",
    "obsolete spelling of",
    "archaic form of",
    "archaic spelling of",
    "dated form of",
    "dated spelling of",
    "plural of",
    "genitive of",
    "dative of",
    "accusative of",
    "inflection of",
    "romanization of",
    "clipping of",
    "participle of",
    "gerund of",
    "supine of",
    "infinitive of",
    "verbal noun of",
];

/// True if a wiktextract sense describes a variant of another entry rather
/// than a meaning of its own (form_of/alt_of fields or junk tags).
fn sense_is_junk(sense: &Value) -> bool {
    if sense.get("form_of").is_some() || sense.get("alt_of").is_some() {
        return true;
    }
    if let Some(Value::Array(tags)) = sense.get("tags") {
        for tag in tags {
            if let Value::String(t) = tag {
                if JUNK_SENSE_TAGS.contains(&t.as_str()) {
                    return true;
                }
            }
        }
    }
    false
}

/// True if a cleaned (lowercased, comma-free) gloss text is bookkeeping.
fn gloss_is_junk(cleaned: &str) -> bool {
    JUNK_GLOSS_MARKERS.iter().any(|m| cleaned.contains(m))
}

/// Register/style tags worth keeping (metaphor & poetry live here);
/// "figuratively" is normalized to "figurative".
const REGISTER_TAGS: &[&str] = &["figurative", "figuratively", "poetic", "literary", "archaic", "dated"];

/// Collect register tags of one sense (normalized).
fn sense_registers(sense: &Value) -> Vec<&'static str> {
    let mut out = Vec::new();
    if let Some(Value::Array(tags)) = sense.get("tags") {
        for tag in tags {
            if let Value::String(t) = tag {
                if let Some(r) = REGISTER_TAGS.iter().find(|r| **r == t.as_str()) {
                    out.push(if *r == "figuratively" { "figurative" } else { *r });
                }
            }
        }
    }
    out
}

/// Wiktextract link fields that become nexus edges, with relation name and
/// base weight. Relation names are the German labels shown in the Spur.
const EDGE_FIELDS: &[(&str, &str, f64)] = &[
    ("synonyms", "synonym", 0.8),
    ("related", "verwandt", 0.6),
    ("derived", "ableitung", 0.5),
    ("antonyms", "gegenteil", 0.4),
    ("coordinate_terms", "nachbar", 0.5),
];

/// Max edges taken per field per entry, to keep prolific entries in check.
const MAX_EDGES_PER_FIELD: usize = 12;

/// Collect edges from one wiktextract field array into `out`.
fn collect_edge_field(items: &Value, src: &str, rel: &str, weight: f64, spec: &SourceSpec, out: &mut Vec<Edge>) {
    let Value::Array(items) = items else { return };
    for item in items.iter().take(MAX_EDGES_PER_FIELD) {
        if let Some(Value::String(w)) = item.get("word") {
            let w = w.trim();
            if w.is_empty() || w.contains(char::is_whitespace) || w.chars().count() > 30 {
                continue;
            }
            let dst = w.to_lowercase();
            if dst == src {
                continue;
            }
            out.push(Edge {
                src: src.to_string(),
                rel: rel.to_string(),
                dst,
                weight,
                source: Some(spec.id.clone()),
            });
        }
    }
}

/// Harvest association edges from a wiktextract entry: top-level and
/// sense-level synonyms/related/derived/antonyms/coordinate_terms.
fn extract_edges(json: &Value, word: &str, spec: &SourceSpec) -> Vec<Edge> {
    let src = word.to_lowercase();
    let mut out = Vec::new();
    for (field, rel, weight) in EDGE_FIELDS {
        if let Some(items) = json.get(*field) {
            collect_edge_field(items, &src, rel, *weight, spec, &mut out);
        }
    }
    if let Some(Value::Array(senses)) = json.get("senses") {
        for sense in senses {
            for (field, rel, weight) in EDGE_FIELDS {
                if let Some(items) = sense.get(*field) {
                    collect_edge_field(items, &src, rel, *weight, spec, &mut out);
                }
            }
        }
    }
    out.sort_by(|a, b| (&a.rel, &a.dst).cmp(&(&b.rel, &b.dst)));
    out.dedup_by(|a, b| a.rel == b.rel && a.dst == b.dst);
    out
}

/// Extract the first romanization from a wiktextract `forms` array
/// (entries whose tags contain "romanization"), e.g. Hebrew and Greek
/// dumps carry the Latin form of the headword there.
fn extract_romanization(json: &Value) -> Option<String> {
    if let Some(Value::Array(forms)) = json.get("forms") {
        for form in forms {
            let is_romanization = matches!(form.get("tags"), Some(Value::Array(tags))
                if tags.iter().any(|t| matches!(t, Value::String(s) if s == "romanization")));
            if !is_romanization {
                continue;
            }
            if let Some(Value::String(f)) = form.get("form") {
                let trimmed = f.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

/// Truncate a string to a maximum byte length, ensuring we break on a char boundary.
/// Returns the truncated string (never panics, even with multibyte chars near boundary).
fn truncate_on_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    let bytes = s.as_bytes();
    let mut end = max_bytes.min(bytes.len());

    // Walk backwards to find a valid UTF-8 char boundary
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    &s[..end]
}

/// Parse ONE wiktextract JSONL line into a ParsedLine (WordRecord + nexus
/// edges) for the given source.
///
/// Returns `Ok(None)` if the line is valid JSON but filtered out (e.g., non-noun word,
/// multiword entry, missing word field, etc.).
///
/// Mapping rules:
/// - word: taken from json["word"] as str. Skipped if missing, contains whitespace,
///   or character count is < 2 or > 30.
/// - pos (word_class): "noun" -> "noun", "adj" -> "adj", "name" -> "proper".
///   Any other pos value is skipped (returns Ok(None)).
/// - language: spec.language
/// - system: spec.system
/// - source: spec.id
/// - seed_weight: 1.0
/// - origin_lang: None
/// - tags: The first 2 USABLE glosses across all senses (variant senses and
///   bookkeeping glosses are skipped, see JUNK_SENSE_TAGS/JUNK_GLOSS_MARKERS),
///   lowercased, inner commas removed, truncated to 80 chars on a char
///   boundary, joined with ",". NO usable gloss → the whole record is skipped.
/// - translit: first romanization from json["forms"] (tags contain
///   "romanization"); falls back to rule-based transliteration for
///   non-Latin words (crate::translit), else None.
/// - registers: union of register tags (figurative/poetic/literary/archaic/
///   dated) of the senses whose glosses were kept; None if none.
/// - edges: association links (synonyms/related/derived/antonyms/
///   coordinate_terms, top-level and per sense) as `src --rel--> dst`.
/// - etymology: json["etymology_text"] truncated to 160 chars on a char boundary.
///   None if absent or empty.
/// - id: format!("{}_{}", spec.language, word)
pub fn parse_wiktextract_line(line: &str, spec: &SourceSpec) -> anyhow::Result<Option<ParsedLine>> {
    let json: Value = serde_json::from_str(line)?;

    // Extract word field
    let word = match json.get("word") {
        Some(Value::String(w)) => w.clone(),
        _ => return Ok(None), // Missing or not a string
    };

    // Validate word
    let char_count = word.chars().count();
    if char_count < 2 || char_count > 30 || word.contains(|c: char| c.is_whitespace()) {
        return Ok(None);
    }

    // Map pos to word_class
    let word_class = match json.get("pos") {
        Some(Value::String(pos)) => {
            match pos.as_str() {
                "noun" => Some("noun".to_string()),
                "adj" => Some("adj".to_string()),
                "name" => Some("proper".to_string()),
                _ => return Ok(None), // Unknown pos, skip this record
            }
        }
        _ => None, // Missing pos
    };

    // Check if the mapped word_class is in skip_classes
    if let Some(ref wc) = word_class {
        if spec.skip_classes.contains(wc) {
            return Ok(None);
        }
    }

    // Extract usable glosses for tags: scan ALL senses (not just the first
    // two), skipping variant senses and bookkeeping glosses, and keep the
    // first 2 real meanings. A record without at least one usable gloss is
    // dictionary bookkeeping (inflections, spelling variants, surnames) and
    // useless for both the concept bridge and the explain output — skip it.
    let mut glosses = Vec::new();
    let mut registers: Vec<&'static str> = Vec::new();
    if let Some(Value::Array(senses)) = json.get("senses") {
        for sense in senses {
            if glosses.len() >= 2 {
                break;
            }
            if sense_is_junk(sense) {
                continue;
            }
            if let Some(Value::Array(sense_glosses)) = sense.get("glosses") {
                if let Some(Value::String(gloss)) = sense_glosses.get(0) {
                    // Remove inner commas, normalize whitespace, lowercase, truncate to 80 chars
                    let cleaned = normalize_whitespace(&gloss.replace(',', "")).to_lowercase();
                    if gloss_is_junk(&cleaned) {
                        continue;
                    }
                    let truncated = truncate_on_char_boundary(&cleaned, 80);
                    glosses.push(truncated.to_string());
                    // Union of register tags of the KEPT senses only
                    for r in sense_registers(sense) {
                        if !registers.contains(&r) {
                            registers.push(r);
                        }
                    }
                }
            }
        }
    }

    if glosses.is_empty() {
        return Ok(None);
    }
    let tags = Some(glosses.join(","));
    let registers = if registers.is_empty() {
        None
    } else {
        Some(registers.join(","))
    };

    // Extract etymology
    let etymology = match json.get("etymology_text") {
        Some(Value::String(et)) => {
            let normalized = normalize_whitespace(et).to_lowercase();
            if normalized.is_empty() {
                None
            } else {
                Some(truncate_on_char_boundary(&normalized, 160).to_string())
            }
        }
        _ => None,
    };

    let id = format!("{}_{}", spec.language, word);

    // Romanization: prefer the dump's own (forms tagged "romanization"),
    // fall back to rule-based transliteration for non-Latin words.
    let translit = extract_romanization(&json)
        .or_else(|| crate::translit::to_latin(&word, Some(spec.language.as_str())));

    let edges = extract_edges(&json, &word, spec);

    Ok(Some(ParsedLine {
        record: WordRecord {
            id,
            word,
            word_class,
            language: Some(spec.language.clone()),
            system: Some(spec.system.clone()),
            tags,
            seed_weight: 1.0,
            source: Some(spec.id.clone()),
            etymology,
            origin_lang: None,
            translit,
            registers,
        },
        edges,
    }))
}

/// Parse the greek-mythology-data JSON body (one array of figures) into
/// WordRecords. Mapping: name → word (proper), category + first part of the
/// description → glosses (the bridge food), greekName/romanName → etymology
/// line. Returns (records, skipped).
pub fn parse_mythology_json(body: &str, spec: &SourceSpec) -> anyhow::Result<(Vec<WordRecord>, usize)> {
    let items: Vec<Value> = serde_json::from_str(body)
        .with_context(|| "failed to parse mythology JSON as array")?;

    let mut records = Vec::new();
    let mut skipped = 0usize;

    for item in &items {
        if records.len() >= spec.max_words {
            break;
        }

        let name = match item.get("name") {
            Some(Value::String(n)) => n.trim().to_string(),
            _ => {
                skipped += 1;
                continue;
            }
        };
        let char_count = name.chars().count();
        if char_count < 2 || char_count > 30 || name.contains(char::is_whitespace) {
            skipped += 1;
            continue;
        }

        let mut glosses = Vec::new();
        if let Some(Value::String(cat)) = item.get("category") {
            let cleaned = normalize_whitespace(&cat.replace(',', "")).to_lowercase();
            if !cleaned.is_empty() {
                glosses.push(cleaned);
            }
        }
        if let Some(Value::String(desc)) = item.get("description") {
            let cleaned = normalize_whitespace(&desc.replace(',', "")).to_lowercase();
            if !cleaned.is_empty() {
                glosses.push(truncate_on_char_boundary(&cleaned, 160).to_string());
            }
        }
        if glosses.is_empty() {
            skipped += 1;
            continue;
        }

        // Origin line for the Wurzel output: greek and roman name forms
        let mut origin_parts = Vec::new();
        if let Some(Value::String(g)) = item.get("greekName") {
            if !g.trim().is_empty() {
                origin_parts.push(format!("griech. {}", g.trim()));
            }
        }
        if let Some(Value::String(r)) = item.get("romanName") {
            if !r.trim().is_empty() {
                origin_parts.push(format!("roem. {}", r.trim()));
            }
        }
        let etymology = if origin_parts.is_empty() {
            None
        } else {
            Some(truncate_on_char_boundary(&origin_parts.join(", "), 160).to_string())
        };

        records.push(WordRecord {
            id: format!("{}_{}", spec.language, name),
            word: name,
            word_class: Some("proper".to_string()),
            language: Some(spec.language.clone()),
            system: Some(spec.system.clone()),
            tags: Some(glosses.join(",")),
            seed_weight: 1.1,
            source: Some(spec.id.clone()),
            etymology,
            origin_lang: Some("grc".to_string()),
            translit: None,
            registers: None,
        });
    }

    Ok((records, skipped))
}

/// A `Read` wrapper that counts the number of bytes pulled through it into a
/// shared atomic counter. Used to report download progress ("MB read") while
/// streaming, without ever buffering the whole body in memory.
pub struct CountingReader<R> {
    inner: R,
    counter: Arc<AtomicU64>,
}

impl<R> CountingReader<R> {
    pub fn new(inner: R, counter: Arc<AtomicU64>) -> Self {
        Self { inner, counter }
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.counter.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

/// Outcome of `fetch_all`: one report per source plus the total number of
/// records and nexus edges actually inserted into the database.
pub struct FetchOutcome {
    pub reports: Vec<FetchReport>,
    pub total_inserted: usize,
    pub total_edges: usize,
}

/// UI-agnostic progress sink for the fetch engine. Implementations must be
/// `Sync` because worker threads call these methods directly (they never
/// touch the `Db`, so this is safe even though `Db` itself is single-writer).
pub trait FetchProgress: Sync {
    fn on_update(&self, id: &str, bytes: u64, accepted: usize, skipped: usize);
    fn on_done(&self, id: &str, report: &FetchReport);
    fn on_error(&self, id: &str, msg: &str);
}

const BATCH_SIZE: usize = 100;
const EDGE_BATCH_SIZE: usize = 400;
const PROGRESS_EVERY_LINES: usize = 50;
const PROGRESS_EVERY: Duration = Duration::from_millis(500);

/// Read the JSONL body from `reader` line by line, parsing each line for
/// `spec` and handing accepted records to `on_batch` in batches of
/// `BATCH_SIZE` (plus a final partial batch). Stops as soon as
/// `spec.max_words` records have been accepted, WITHOUT reading the rest of
/// the stream (early termination is the point: source files are GB-sized).
///
/// `bytes_read` is called to obtain the current byte count for progress
/// reporting; it is decoupled from `R` so tests can pass a fixed/derived
/// value instead of wiring up a real byte counter.
///
/// Broken JSON lines are treated as skippable, not fatal: this function
/// never returns an `Err` from a single bad line.
pub fn consume_jsonl<R: Read>(
    reader: R,
    spec: &SourceSpec,
    bytes_read: impl Fn() -> u64,
    mut on_progress: impl FnMut(u64, usize, usize),
    mut on_batch: impl FnMut(Vec<WordRecord>),
    mut on_edges: impl FnMut(Vec<Edge>),
) -> FetchReport {
    let mut accepted = 0usize;
    let mut skipped = 0usize;
    let mut edge_count = 0usize;
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    let mut edge_batch: Vec<Edge> = Vec::with_capacity(EDGE_BATCH_SIZE);
    let mut lines_since_report = 0usize;
    let mut last_report = Instant::now();

    let buf_reader = BufReader::new(reader);
    for line in buf_reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // underlying stream broke; stop, report what we have
        };
        if line.trim().is_empty() {
            continue;
        }

        match parse_wiktextract_line(&line, spec) {
            Ok(Some(parsed)) => {
                accepted += 1;
                batch.push(parsed.record);
                edge_count += parsed.edges.len();
                edge_batch.extend(parsed.edges);
                if batch.len() >= BATCH_SIZE {
                    on_batch(std::mem::replace(&mut batch, Vec::with_capacity(BATCH_SIZE)));
                }
                if edge_batch.len() >= EDGE_BATCH_SIZE {
                    on_edges(std::mem::replace(&mut edge_batch, Vec::with_capacity(EDGE_BATCH_SIZE)));
                }
            }
            Ok(None) => skipped += 1,
            Err(_) => skipped += 1, // broken-but-skippable line
        }

        lines_since_report += 1;
        if lines_since_report >= PROGRESS_EVERY_LINES || last_report.elapsed() >= PROGRESS_EVERY {
            on_progress(bytes_read(), accepted, skipped);
            lines_since_report = 0;
            last_report = Instant::now();
        }

        if accepted >= spec.max_words {
            break;
        }
    }

    if !batch.is_empty() {
        on_batch(batch);
    }
    if !edge_batch.is_empty() {
        on_edges(edge_batch);
    }

    let final_bytes = bytes_read();
    on_progress(final_bytes, accepted, skipped);

    FetchReport {
        id: spec.id.clone(),
        accepted,
        skipped,
        edges: edge_count,
        bytes_read: final_bytes,
        error: None,
    }
}

/// Offline-testable parse of a Datamuse response body: JSON array of {"word": ..., "score": ...}.
/// Returns at most max words, single words only (skip entries containing whitespace).
pub fn parse_datamuse_response(body: &str, max: usize) -> anyhow::Result<Vec<String>> {
    let array: Vec<serde_json::Value> = serde_json::from_str(body)
        .with_context(|| "failed to parse Datamuse response as JSON array")?;

    let mut words = Vec::new();
    for item in array.iter().take(max * 2) {
        // Extract "word" field
        if let Some(serde_json::Value::String(w)) = item.get("word") {
            // Only include single words (no whitespace)
            if !w.contains(char::is_whitespace) {
                words.push(w.clone());
                if words.len() >= max {
                    break;
                }
            }
        }
    }

    Ok(words)
}

/// How many association triggers (rel_trg) to request per query token, and
/// for how many tokens. Triggers are Datamuse's statistical associations —
/// the closest thing to the "metaphor jump" the North Star asks for.
const TRIGGERS_PER_TOKEN: usize = 5;
const MAX_TRIGGER_TOKENS: usize = 3;

/// Fast connectivity probe against the expansion endpoint: one cheap GET
/// with a short timeout. Used to decide online vs. local mode without
/// making the user wait for full request timeouts when there is no net.
pub fn quick_connectivity_check(url: &str) -> bool {
    ureq::get(url)
        .query("max", "1")
        .timeout(Duration::from_millis(1200))
        .call()
        .is_ok()
}

/// Semantic query expansion via Datamuse (10s timeout per request; .query()
/// handles encoding). Two passes:
/// 1. `ml=<query>` (means-like) over the whole description — fatal on failure.
/// 2. `rel_trg=<token>` (association triggers) for the first content tokens —
///    best-effort: a failing trigger request is silently skipped.
/// Results are deduplicated case-insensitively, ml candidates first.
pub fn expand_query(spec: &crate::sources::QueryExpansionSpec, query: &str) -> anyhow::Result<Vec<String>> {
    match spec.backend {
        crate::sources::ExpansionBackend::DatamuseMl => {
            let mut out = Vec::new();
            let mut seen = std::collections::HashSet::new();

            let response = ureq::get(&spec.url)
                .query("ml", query)
                .query("max", &spec.max_candidates.to_string())
                .timeout(Duration::from_secs(10))
                .call()
                .with_context(|| format!("failed to fetch from Datamuse API for query '{}'", query))?;

            let body = response.into_string()
                .with_context(|| "failed to read Datamuse response body")?;

            for w in parse_datamuse_response(&body, spec.max_candidates)? {
                if seen.insert(w.to_lowercase()) {
                    out.push(w);
                }
            }

            for token in crate::lookup::tokenize(query).into_iter().take(MAX_TRIGGER_TOKENS) {
                let response = ureq::get(&spec.url)
                    .query("rel_trg", &token)
                    .query("max", &TRIGGERS_PER_TOKEN.to_string())
                    .timeout(Duration::from_secs(10))
                    .call();
                let Ok(response) = response else { continue };
                let Ok(body) = response.into_string() else { continue };
                let Ok(words) = parse_datamuse_response(&body, TRIGGERS_PER_TOKEN) else { continue };
                for w in words {
                    if seen.insert(w.to_lowercase()) {
                        out.push(w);
                    }
                }
            }

            Ok(out)
        }
    }
}

/// Open the HTTP(S) body for `spec.url` as a streaming reader, transparently
/// gunzipping if the URL ends in `.gz`. Returns the reader plus a shared byte
/// counter tracking raw (pre-decompression) bytes pulled off the wire.
fn open_source_stream(spec: &SourceSpec) -> anyhow::Result<(Box<dyn Read + Send>, Arc<AtomicU64>)> {
    let response = ureq::get(&spec.url)
        .timeout(Duration::from_secs(60))
        .call()
        .with_context(|| format!("failed to fetch {}", spec.url))?;

    let counter = Arc::new(AtomicU64::new(0));
    let counting = CountingReader::new(response.into_reader(), counter.clone());

    let reader: Box<dyn Read + Send> = if spec.url.ends_with(".gz") {
        Box::new(GzDecoder::new(counting))
    } else {
        Box::new(counting)
    };

    Ok((reader, counter))
}

/// Messages sent from worker threads to the main (Db-owning) thread.
enum Msg {
    Batch(Vec<WordRecord>),
    Edges(Vec<Edge>),
    Done(FetchReport),
    Error { id: String, msg: String },
}

/// Fetch a whole-body JSON source (mythology-json backend): downloads the
/// file, parses it, returns the records and a report. Not streamed — these
/// files are small (single-digit MB).
fn fetch_mythology_source(spec: &SourceSpec) -> anyhow::Result<(Vec<WordRecord>, FetchReport)> {
    let response = ureq::get(&spec.url)
        .timeout(Duration::from_secs(60))
        .call()
        .with_context(|| format!("failed to fetch {}", spec.url))?;
    let body = response
        .into_string()
        .with_context(|| "failed to read mythology JSON body")?;
    let bytes_read = body.len() as u64;

    let (records, skipped) = parse_mythology_json(&body, spec)?;
    let report = FetchReport {
        id: spec.id.clone(),
        accepted: records.len(),
        skipped,
        edges: 0,
        bytes_read,
        error: None,
    };
    Ok((records, report))
}

/// Fetch every source in `specs` in parallel, streaming each into the
/// database. One worker thread per source parses and sends `WordRecord`
/// batches through an mpsc channel; the calling (main) thread is the ONLY
/// thread that touches `db`, inserting each batch inside its own
/// transaction (via `Db::insert_words`, reusing the existing insert path).
///
/// A failing source (bad URL, network error, timeout, ...) does not abort
/// the others: it is reported via `progress.on_error` and its `FetchReport`
/// carries the error message; `fetch_all` itself still returns `Ok`.
pub fn fetch_all(
    db: &mut Db,
    specs: &[SourceSpec],
    progress: &dyn FetchProgress,
) -> anyhow::Result<FetchOutcome> {
    let (tx, rx) = std::sync::mpsc::channel::<Msg>();

    std::thread::scope(|scope| {
        for spec in specs {
            let tx = tx.clone();
            scope.spawn(move || {
                let id = spec.id.clone();
                match spec.backend {
                    crate::sources::Backend::WiktextractJsonl => match open_source_stream(spec) {
                        Ok((reader, counter)) => {
                            let counter_for_bytes = counter.clone();
                            let tx_batch = tx.clone();
                            let tx_edges = tx.clone();
                            let report = consume_jsonl(
                                reader,
                                spec,
                                move || counter_for_bytes.load(Ordering::Relaxed),
                                |bytes, accepted, skipped| {
                                    progress.on_update(&id, bytes, accepted, skipped);
                                },
                                |records| {
                                    let _ = tx_batch.send(Msg::Batch(records));
                                },
                                |edges| {
                                    let _ = tx_edges.send(Msg::Edges(edges));
                                },
                            );
                            let _ = tx.send(Msg::Done(report));
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            progress.on_error(&id, &msg);
                            let _ = tx.send(Msg::Error { id, msg });
                        }
                    },
                    crate::sources::Backend::MythologyJson => match fetch_mythology_source(spec) {
                        Ok((records, report)) => {
                            progress.on_update(&id, report.bytes_read, report.accepted, report.skipped);
                            let _ = tx.send(Msg::Batch(records));
                            let _ = tx.send(Msg::Done(report));
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            progress.on_error(&id, &msg);
                            let _ = tx.send(Msg::Error { id, msg });
                        }
                    },
                }
            });
        }
        // Drop our own sender so `rx` iteration ends once all worker clones
        // (and thus all threads) have finished and dropped theirs.
        drop(tx);

        let mut reports: Vec<FetchReport> = Vec::new();
        let mut total_inserted = 0usize;
        let mut total_edges = 0usize;

        for msg in rx {
            match msg {
                Msg::Batch(records) => {
                    let count = records.len();
                    db.insert_words(&records)?;
                    total_inserted += count;
                }
                Msg::Edges(edges) => {
                    let count = edges.len();
                    db.insert_edges(&edges)?;
                    total_edges += count;
                }
                Msg::Done(report) => {
                    progress.on_done(&report.id, &report);
                    reports.push(report);
                }
                Msg::Error { id, msg } => {
                    let report = FetchReport {
                        id: id.clone(),
                        accepted: 0,
                        skipped: 0,
                        edges: 0,
                        bytes_read: 0,
                        error: Some(msg),
                    };
                    reports.push(report);
                }
            }
        }

        // Preserve the caller's source order in the returned reports.
        reports.sort_by_key(|r| specs.iter().position(|s| s.id == r.id).unwrap_or(usize::MAX));

        Ok(FetchOutcome {
            reports,
            total_inserted,
            total_edges,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_spec() -> SourceSpec {
        SourceSpec {
            id: "test-source".to_string(),
            backend: crate::sources::Backend::WiktextractJsonl,
            url: "https://example.org/test.jsonl".to_string(),
            language: "de".to_string(),
            system: "test_system".to_string(),
            max_words: 100,
            skip_classes: Vec::new(),
        }
    }

    #[test]
    fn test_parse_noun_with_glosses() {
        let spec = make_spec();
        let json_line = r#"{"word":"Test","pos":"noun","senses":[{"glosses":["a trial or examination"]},{"glosses":["evidence"]}],"etymology_text":"from Latin testum"}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse valid JSON")
            .expect("expected Some record")
            .record;

        assert_eq!(result.id, "de_Test");
        assert_eq!(result.word, "Test");
        assert_eq!(result.word_class, Some("noun".to_string()));
        assert_eq!(result.language, Some("de".to_string()));
        assert_eq!(result.system, Some("test_system".to_string()));
        assert_eq!(result.source, Some("test-source".to_string()));
        assert_eq!(result.seed_weight, 1.0);
        assert_eq!(result.origin_lang, None);
        assert_eq!(result.tags, Some("a trial or examination,evidence".to_string()));
        assert_eq!(result.etymology, Some("from latin testum".to_string()));
    }

    #[test]
    fn test_parse_adj_pos() {
        let spec = make_spec();
        let json_line = r#"{"word":"schön","pos":"adj","senses":[{"glosses":["beautiful"]}]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse")
            .expect("expected Some record")
            .record;

        assert_eq!(result.word_class, Some("adj".to_string()));
    }

    #[test]
    fn test_parse_name_pos() {
        let spec = make_spec();
        let json_line = r#"{"word":"Zeus","pos":"name","senses":[{"glosses":["king of the gods"]}]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse")
            .expect("expected Some record")
            .record;

        assert_eq!(result.word_class, Some("proper".to_string()));
    }

    #[test]
    fn test_parse_verb_filtered() {
        let spec = make_spec();
        let json_line = r#"{"word":"laufen","pos":"verb","senses":[]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse");

        assert_eq!(result, None, "verb pos should be filtered out");
    }

    #[test]
    fn test_parse_multiword_filtered() {
        let spec = make_spec();
        let json_line = r#"{"word":"New York","pos":"noun","senses":[]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse");

        assert_eq!(result, None, "multiword should be filtered out");
    }

    #[test]
    fn test_parse_missing_word_filtered() {
        let spec = make_spec();
        let json_line = r#"{"pos":"noun","senses":[]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse");

        assert_eq!(result, None, "missing word should be filtered out");
    }

    #[test]
    fn test_parse_broken_json() {
        let spec = make_spec();
        let json_line = r#"{"word":"test","pos":"noun","senses":[}}"#;

        let result = parse_wiktextract_line(json_line, &spec);
        assert!(result.is_err(), "broken JSON should error");
    }

    #[test]
    fn test_parse_etymology_truncation() {
        let spec = make_spec();
        // Create a long etymology with multibyte chars (German umlauts)
        // Use serde_json to properly escape the string
        let long_text = "Dies ist eine sehr lange Etymologie mit vielen Zeichen und Umlauten äöü die über 160 Zeichen hinausgeht und daher abgeschnitten werden sollte ohne dass dabei die Strings brechen";
        let json_obj = serde_json::json!({
            "word": "Test",
            "pos": "noun",
            "senses": [{"glosses": ["a trial"]}],
            "etymology_text": long_text
        });
        let json_line = json_obj.to_string();

        let result = parse_wiktextract_line(&json_line, &spec)
            .expect("failed to parse")
            .expect("expected Some record")
            .record;

        assert!(result.etymology.is_some());
        let et = result.etymology.unwrap();
        assert!(et.len() <= 160, "etymology should be <= 160 bytes; got {} bytes", et.len());
        // Verify it doesn't panic on char boundaries
        let _ = et.chars().count();
    }

    #[test]
    fn test_parse_tags_truncation() {
        let spec = make_spec();
        let long_gloss = "Dies ist ein sehr langer Glosstext mit vielen Zeichen und Umlauten äöü die über 80 Zeichen hinausgeht und daher abgeschnitten werden sollte";
        let json_obj = serde_json::json!({
            "word": "Test",
            "pos": "noun",
            "senses": [
                {
                    "glosses": [long_gloss]
                }
            ]
        });
        let json_line = json_obj.to_string();

        let result = parse_wiktextract_line(&json_line, &spec)
            .expect("failed to parse")
            .expect("expected Some record")
            .record;

        assert!(result.tags.is_some());
        let tags = result.tags.unwrap();
        // Each tag is truncated to 80 bytes
        for tag in tags.split(',') {
            assert!(tag.len() <= 80, "tag should be <= 80 bytes; got {} bytes: {}", tag.len(), tag);
        }
    }

    #[test]
    fn test_parse_no_glosses_filtered() {
        let spec = make_spec();
        let json_line = r#"{"word":"Test","pos":"noun","senses":[]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse");

        assert_eq!(result, None, "records without usable glosses should be filtered");
    }

    #[test]
    fn test_parse_empty_etymology() {
        let spec = make_spec();
        let json_line = r#"{"word":"Test","pos":"noun","senses":[{"glosses":["a trial"]}],"etymology_text":""}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse")
            .expect("expected Some record")
            .record;

        assert_eq!(result.etymology, None, "empty etymology should be None");
    }

    #[test]
    fn test_parse_junk_gloss_surname_filtered() {
        let spec = make_spec();
        // Comma is stripped during cleaning; "surname" must still be caught
        let json_line = r#"{"word":"Baum","pos":"name","senses":[{"glosses":["A surname, a German Jewish surname"]}]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse");

        assert_eq!(result, None, "surname-only entries should be filtered");
    }

    #[test]
    fn test_parse_junk_sense_form_of_filtered() {
        let spec = make_spec();
        let json_line = r#"{"word":"laudata","pos":"adj","senses":[{"tags":["form-of"],"form_of":[{"word":"laudatus"}],"glosses":["inflection of laudatus"]}]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse");

        assert_eq!(result, None, "pure inflection entries should be filtered");
    }

    #[test]
    fn test_parse_junk_sense_skipped_but_real_sense_kept() {
        let spec = make_spec();
        // First sense is an alternative form, second carries a real meaning
        let json_line = r#"{"word":"Wald","pos":"noun","senses":[{"glosses":["alternative form of Walde"]},{"glosses":["forest, woodland"]}]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse")
            .expect("expected Some record")
            .record;

        assert_eq!(result.tags, Some("forest woodland".to_string()));
    }

    #[test]
    fn test_parse_word_too_short() {
        let spec = make_spec();
        let json_line = r#"{"word":"a","pos":"noun","senses":[]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse");

        assert_eq!(result, None, "single char word should be filtered");
    }

    #[test]
    fn test_parse_word_too_long() {
        let spec = make_spec();
        // 31 characters
        let long_word = "abcdefghijklmnopqrstuvwxyzabcde";
        let json_line = format!(
            r#"{{"word":"{}","pos":"noun","senses":[]}}"#,
            long_word
        );

        let result = parse_wiktextract_line(&json_line, &spec)
            .expect("failed to parse");

        assert_eq!(result, None, "word with > 30 chars should be filtered");
    }

    #[test]
    fn test_parse_skip_classes_proper() {
        let mut spec = make_spec();
        spec.skip_classes = vec!["proper".to_string()];
        let json_line = r#"{"word":"Maria","pos":"name","senses":[]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse");

        assert_eq!(result, None, "proper class should be filtered when in skip_classes");
    }

    #[test]
    fn test_parse_skip_classes_noun() {
        let mut spec = make_spec();
        spec.skip_classes = vec!["noun".to_string()];
        let json_line = r#"{"word":"Test","pos":"noun","senses":[]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse");

        assert_eq!(result, None, "noun class should be filtered when in skip_classes");
    }

    #[test]
    fn test_parse_skip_classes_empty_allows_all() {
        let spec = make_spec(); // skip_classes is empty by default
        let json_line = r#"{"word":"Test","pos":"noun","senses":[{"glosses":["a trial"]}]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse");

        assert!(result.is_some(), "empty skip_classes should allow all classes");
    }

    #[test]
    fn test_parse_extracts_edges_from_synonyms_and_related() {
        let spec = make_spec();
        let json_line = r#"{"word":"Spur","pos":"noun","senses":[{"glosses":["a trace or track"],"synonyms":[{"word":"Fährte"}]}],"related":[{"word":"Weg"},{"word":"New York"}],"synonyms":[{"word":"Zeichen"}]}"#;

        let parsed = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse")
            .expect("expected Some");

        let rels: Vec<(String, String)> = parsed
            .edges
            .iter()
            .map(|e| (e.rel.clone(), e.dst.clone()))
            .collect();
        assert!(rels.contains(&("synonym".to_string(), "fährte".to_string())));
        assert!(rels.contains(&("synonym".to_string(), "zeichen".to_string())));
        assert!(rels.contains(&("verwandt".to_string(), "weg".to_string())));
        // Multiword targets are dropped
        assert!(!rels.iter().any(|(_, d)| d.contains("new")));
        // src is the lowercased headword
        assert!(parsed.edges.iter().all(|e| e.src == "spur"));
    }

    #[test]
    fn test_parse_registers_from_kept_senses_only() {
        let spec = make_spec();
        // First sense junk (form-of), second poetic+figurative, third plain
        let json_line = r#"{"word":"Faden","pos":"noun","senses":[{"tags":["form-of"],"form_of":[{"word":"Fäden"}],"glosses":["inflection of Fäden"]},{"tags":["figuratively","poetic"],"glosses":["a connecting thread of a narrative"]},{"glosses":["a thin thread"]}]}"#;

        let parsed = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse")
            .expect("expected Some");

        assert_eq!(parsed.record.registers, Some("figurative,poetic".to_string()));
        // Junk sense gloss was skipped, both real glosses kept
        assert_eq!(
            parsed.record.tags,
            Some("a connecting thread of a narrative,a thin thread".to_string())
        );
    }

    #[test]
    fn test_parse_mythology_json_maps_figures() {
        let spec = SourceSpec {
            id: "myth-greek".to_string(),
            backend: crate::sources::Backend::MythologyJson,
            url: "https://example.org/all.json".to_string(),
            language: "grc".to_string(),
            system: "myth_greek".to_string(),
            max_words: 100,
            skip_classes: Vec::new(),
        };
        let body = r#"[
            {"name":"Aphrodite","greekName":"Ἀφροδίτη, Aphroditē","romanName":"Venus","category":"major olympians","description":"Goddess of beauty, love, desire, and pleasure."},
            {"name":"Cronus","greekName":"Κρόνος (Kronos)","category":"twelve titan","description":"Titan of harvests."},
            {"name":"No Description","category":"","description":""}
        ]"#;

        let (records, skipped) = parse_mythology_json(body, &spec).expect("failed to parse");

        assert_eq!(records.len(), 2);
        assert_eq!(skipped, 1, "entry without glosses is skipped");
        let aph = &records[0];
        assert_eq!(aph.word, "Aphrodite");
        assert_eq!(aph.id, "grc_Aphrodite");
        assert_eq!(aph.word_class, Some("proper".to_string()));
        assert_eq!(aph.system, Some("myth_greek".to_string()));
        let tags = aph.tags.as_deref().unwrap();
        assert!(tags.contains("major olympians"));
        assert!(tags.contains("goddess of beauty love desire and pleasure."));
        let ety = aph.etymology.as_deref().unwrap();
        assert!(ety.contains("griech. Ἀφροδίτη"));
        assert!(ety.contains("roem. Venus"));
    }

    #[test]
    fn test_parse_datamuse_response_three_words() {
        let body = r#"[
            {"word": "forest", "score": 100},
            {"word": "tree canopy", "score": 90},
            {"word": "woodland", "score": 80}
        ]"#;

        let result = parse_datamuse_response(body, 10)
            .expect("failed to parse Datamuse response");

        // Should contain forest and woodland, but not "tree canopy" (has whitespace)
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"forest".to_string()));
        assert!(result.contains(&"woodland".to_string()));
    }

    #[test]
    fn test_parse_datamuse_response_respects_max() {
        let body = r#"[
            {"word": "forest", "score": 100},
            {"word": "woodland", "score": 90},
            {"word": "grove", "score": 80},
            {"word": "canopy", "score": 70}
        ]"#;

        let result = parse_datamuse_response(body, 2)
            .expect("failed to parse Datamuse response");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "forest");
        assert_eq!(result[1], "woodland");
    }

    #[test]
    fn test_parse_datamuse_response_broken_json() {
        let body = r#"[{"word": "forest", "score": 100,]"#;

        let result = parse_datamuse_response(body, 10);
        assert!(result.is_err(), "broken JSON should return error");
    }

    #[test]
    #[ignore]
    fn test_expand_query_real_api() {
        let spec = crate::sources::QueryExpansionSpec {
            backend: crate::sources::ExpansionBackend::DatamuseMl,
            url: "https://api.datamuse.com/words".to_string(),
            max_candidates: 5,
        };

        let result = expand_query(&spec, "tree")
            .expect("failed to expand query");

        assert!(!result.is_empty(), "should return non-empty candidates for 'tree'");
    }
}
