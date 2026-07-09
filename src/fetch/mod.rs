use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use flate2::read::GzDecoder;
use serde_json::Value;

use crate::db::Db;
use crate::{WordRecord, sources::SourceSpec};

/// Report from processing a single source.
#[derive(Debug, Clone)]
pub struct FetchReport {
    pub id: String,
    pub accepted: usize,
    pub skipped: usize,
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

/// Parse ONE wiktextract JSONL line into a WordRecord for the given source.
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
/// - tags: Up to the first 2 glosses from json["senses"][0..]["glosses"][0],
///   lowercased, inner commas removed, truncated to 80 chars on a char boundary,
///   joined with ",". None if no glosses.
/// - etymology: json["etymology_text"] truncated to 160 chars on a char boundary.
///   None if absent or empty.
/// - id: format!("{}_{}", spec.language, word)
pub fn parse_wiktextract_line(line: &str, spec: &SourceSpec) -> anyhow::Result<Option<WordRecord>> {
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

    // Extract glosses for tags
    let mut glosses = Vec::new();
    if let Some(Value::Array(senses)) = json.get("senses") {
        for sense in senses.iter().take(2) {
            if let Some(Value::Array(sense_glosses)) = sense.get("glosses") {
                if let Some(Value::String(gloss)) = sense_glosses.get(0) {
                    // Remove inner commas, normalize whitespace, lowercase, truncate to 80 chars
                    let cleaned = normalize_whitespace(&gloss.replace(',', "")).to_lowercase();
                    let truncated = truncate_on_char_boundary(&cleaned, 80);
                    glosses.push(truncated.to_string());
                }
            }
        }
    }

    let tags = if glosses.is_empty() {
        None
    } else {
        Some(glosses.join(","))
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

    Ok(Some(WordRecord {
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
    }))
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
/// records actually inserted into the database.
pub struct FetchOutcome {
    pub reports: Vec<FetchReport>,
    pub total_inserted: usize,
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
) -> FetchReport {
    let mut accepted = 0usize;
    let mut skipped = 0usize;
    let mut batch = Vec::with_capacity(BATCH_SIZE);
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
            Ok(Some(rec)) => {
                accepted += 1;
                batch.push(rec);
                if batch.len() >= BATCH_SIZE {
                    on_batch(std::mem::replace(&mut batch, Vec::with_capacity(BATCH_SIZE)));
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

    let final_bytes = bytes_read();
    on_progress(final_bytes, accepted, skipped);

    FetchReport {
        id: spec.id.clone(),
        accepted,
        skipped,
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

/// GET <spec.url>?ml=<query>&max=<max_candidates> with a 10s timeout via ureq (.query() handles encoding).
pub fn expand_query(spec: &crate::sources::QueryExpansionSpec, query: &str) -> anyhow::Result<Vec<String>> {
    match spec.backend {
        crate::sources::ExpansionBackend::DatamuseMl => {
            let response = ureq::get(&spec.url)
                .query("ml", query)
                .query("max", &spec.max_candidates.to_string())
                .timeout(Duration::from_secs(10))
                .call()
                .with_context(|| format!("failed to fetch from Datamuse API for query '{}'", query))?;

            let body = response.into_string()
                .with_context(|| "failed to read Datamuse response body")?;

            parse_datamuse_response(&body, spec.max_candidates)
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
    Done(FetchReport),
    Error { id: String, msg: String },
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
                match open_source_stream(spec) {
                    Ok((reader, counter)) => {
                        let counter_for_bytes = counter.clone();
                        let tx_batch = tx.clone();
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
                        );
                        let _ = tx.send(Msg::Done(report));
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        progress.on_error(&id, &msg);
                        let _ = tx.send(Msg::Error { id, msg });
                    }
                }
            });
        }
        // Drop our own sender so `rx` iteration ends once all worker clones
        // (and thus all threads) have finished and dropped theirs.
        drop(tx);

        let mut reports: Vec<FetchReport> = Vec::new();
        let mut total_inserted = 0usize;

        for msg in rx {
            match msg {
                Msg::Batch(records) => {
                    let count = records.len();
                    db.insert_words(&records)?;
                    total_inserted += count;
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
            .expect("expected Some record");

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
            .expect("expected Some record");

        assert_eq!(result.word_class, Some("adj".to_string()));
    }

    #[test]
    fn test_parse_name_pos() {
        let spec = make_spec();
        let json_line = r#"{"word":"Maria","pos":"name","senses":[]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse")
            .expect("expected Some record");

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
            "senses": [],
            "etymology_text": long_text
        });
        let json_line = json_obj.to_string();

        let result = parse_wiktextract_line(&json_line, &spec)
            .expect("failed to parse")
            .expect("expected Some record");

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
            .expect("expected Some record");

        assert!(result.tags.is_some());
        let tags = result.tags.unwrap();
        // Each tag is truncated to 80 bytes
        for tag in tags.split(',') {
            assert!(tag.len() <= 80, "tag should be <= 80 bytes; got {} bytes: {}", tag.len(), tag);
        }
    }

    #[test]
    fn test_parse_no_glosses() {
        let spec = make_spec();
        let json_line = r#"{"word":"Test","pos":"noun","senses":[]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse")
            .expect("expected Some record");

        assert_eq!(result.tags, None, "no glosses should result in no tags");
    }

    #[test]
    fn test_parse_empty_etymology() {
        let spec = make_spec();
        let json_line = r#"{"word":"Test","pos":"noun","senses":[],"etymology_text":""}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse")
            .expect("expected Some record");

        assert_eq!(result.etymology, None, "empty etymology should be None");
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
        let json_line = r#"{"word":"Test","pos":"noun","senses":[]}"#;

        let result = parse_wiktextract_line(json_line, &spec)
            .expect("failed to parse");

        assert!(result.is_some(), "empty skip_classes should allow all classes");
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
