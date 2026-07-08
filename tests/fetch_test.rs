use spoor::fetch::{parse_wiktextract_line, consume_jsonl, CountingReader, fetch_all, FetchProgress, FetchReport};
use spoor::sources::{load_sources, SourceSpec, Backend};
use spoor::Db;
use tempfile::NamedTempFile;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

// Helper to create a test SourceSpec
fn make_spec(id: &str, lang: &str) -> SourceSpec {
    SourceSpec {
        id: id.to_string(),
        backend: Backend::WiktextractJsonl,
        url: "https://example.org/test.jsonl".to_string(),
        language: lang.to_string(),
        system: format!("test_{}", lang),
        max_words: 100,
    }
}

#[test]
fn test_parse_realistic_noun() {
    let spec = make_spec("test-de", "de");
    let json_line = r#"
    {
        "word": "Haus",
        "pos": "noun",
        "senses": [
            {
                "glosses": ["a building for human habitation"]
            },
            {
                "glosses": ["a dynasty or family"]
            }
        ],
        "etymology_text": "from Old High German hus"
    }
    "#.trim();

    let result = parse_wiktextract_line(json_line, &spec)
        .expect("failed to parse valid JSON")
        .expect("expected Some record");

    assert_eq!(result.id, "de_Haus");
    assert_eq!(result.word, "Haus");
    assert_eq!(result.word_class, Some("noun".to_string()));
    assert_eq!(result.language, Some("de".to_string()));
    assert_eq!(result.system, Some("test_de".to_string()));
    assert_eq!(result.source, Some("test-de".to_string()));
    assert_eq!(result.seed_weight, 1.0);
    assert_eq!(result.origin_lang, None);
    // Tags should contain first 2 glosses, lowercased, commas removed, truncated
    assert!(result.tags.is_some());
    let tags = result.tags.unwrap();
    assert!(tags.contains("a building for human habitation"));
    assert!(tags.contains("a dynasty or family"));
    // Etymology should be lowercased and truncated
    assert_eq!(result.etymology, Some("from old high german hus".to_string()));
}

#[test]
fn test_parse_verb_returns_none() {
    let spec = make_spec("test-de", "de");
    let json_line = r#"{"word":"laufen","pos":"verb","senses":[]}"#;

    let result = parse_wiktextract_line(json_line, &spec)
        .expect("failed to parse");

    assert_eq!(result, None, "verb pos should be filtered out");
}

#[test]
fn test_parse_multiword_returns_none() {
    let spec = make_spec("test-de", "de");
    let json_line = r#"{"word":"New York","pos":"noun","senses":[]}"#;

    let result = parse_wiktextract_line(json_line, &spec)
        .expect("failed to parse");

    assert_eq!(result, None, "multiword should be filtered out");
}

#[test]
fn test_parse_missing_word_returns_none() {
    let spec = make_spec("test-de", "de");
    let json_line = r#"{"pos":"noun","senses":[]}"#;

    let result = parse_wiktextract_line(json_line, &spec)
        .expect("failed to parse");

    assert_eq!(result, None, "missing word should be filtered out");
}

#[test]
fn test_parse_broken_json_returns_error() {
    let spec = make_spec("test-de", "de");
    let json_line = r#"{"word":"test","pos":"noun","senses":[}"#;

    let result = parse_wiktextract_line(json_line, &spec);
    assert!(result.is_err(), "broken JSON should return error");
}

#[test]
fn test_parse_etymology_truncation_with_multibyte() {
    let spec = make_spec("test-de", "de");
    // Create etymology with German umlauts that spans more than 160 bytes
    let long_text = "Dies ist eine sehr lange Etymologie mit vielen Zeichen und Umlauten äöü die über 160 Zeichen hinausgeht und daher abgeschnitten werden sollte ohne dass dabei die UTF-8 Sequenzen brechen";
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
    // Must be <= 160 bytes
    assert!(et.len() <= 160, "etymology should be <= 160 bytes; got {} bytes", et.len());
    // Verify it's valid UTF-8 (no panic on char iteration)
    let _ = et.chars().count();
}

#[test]
fn test_load_sources_with_comment_header() {
    let yaml_content = r#"# Wortquellen fuer 'spoor db fetch'.
#
# WICHTIG: 'backend' bestimmt den Parser im Code. Es existiert NUR fuer die
# folgenden Backend-Typen eine Implementierung - andere Werte schlagen beim
# Laden fehl:
#   - wiktextract-jsonl   (kaikki.org JSONL-Exporte, eine JSON-Zeile pro Wort)
sources:
  - id: kaikki-de
    backend: wiktextract-jsonl
    url: https://example.org/de.jsonl
    language: de
    system: wiktionary_de
    max_words: 250
  - id: kaikki-en
    backend: wiktextract-jsonl
    url: https://example.org/en.jsonl
    language: en
    system: wiktionary_en
  - id: kaikki-la
    backend: wiktextract-jsonl
    url: https://example.org/la.jsonl
    language: la
    system: wiktionary_la
    max_words: 300
"#;

    let mut file = NamedTempFile::new().unwrap();
    file.write_all(yaml_content.as_bytes()).unwrap();
    file.flush().unwrap();

    let config = load_sources(file.path()).expect("failed to load valid YAML");
    assert_eq!(config.sources.len(), 3);

    // Check first spec
    assert_eq!(config.sources[0].id, "kaikki-de");
    assert_eq!(config.sources[0].language, "de");
    assert_eq!(config.sources[0].max_words, 250);

    // Check second spec (no explicit max_words, should default to 500)
    assert_eq!(config.sources[1].id, "kaikki-en");
    assert_eq!(config.sources[1].max_words, 500);

    // Check third spec
    assert_eq!(config.sources[2].id, "kaikki-la");
    assert_eq!(config.sources[2].max_words, 300);
}

#[test]
fn test_load_sources_unknown_backend_error() {
    let yaml_content = r#"
sources:
  - id: bad-source
    backend: unknown-parser-type
    url: https://example.org/words.jsonl
    language: xx
    system: bad_system
"#;

    let mut file = NamedTempFile::new().unwrap();
    file.write_all(yaml_content.as_bytes()).unwrap();
    file.flush().unwrap();

    let result = load_sources(file.path());
    assert!(result.is_err(), "should fail with unknown backend");

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("wiktextract-jsonl"),
        "error message should list supported backends; got: {}",
        err_msg
    );
}

/// A `Read` implementation that yields at most one byte per call, regardless
/// of the buffer size requested. This lets us prove that `consume_jsonl`
/// stops reading as soon as `max_words` is reached, instead of a `BufReader`
/// hiding early termination behind a single large `fill_buf` call.
struct ByteAtATimeReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Read for ByteAtATimeReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.data.len() || buf.is_empty() {
            return Ok(0);
        }
        buf[0] = self.data[self.pos];
        self.pos += 1;
        Ok(1)
    }
}

#[test]
fn test_consume_jsonl_stops_early_at_max_words() {
    let spec = make_spec("test-early-stop", "en");
    // 3 valid nouns, then 1 verb (skippable), then 1 broken JSON (skippable-but-erroring).
    let lines = vec![
        r#"{"word":"alpha","pos":"noun","senses":[]}"#.to_string(),
        r#"{"word":"beta","pos":"noun","senses":[]}"#.to_string(),
        r#"{"word":"gamma","pos":"noun","senses":[]}"#.to_string(),
        r#"{"word":"walk","pos":"verb","senses":[]}"#.to_string(),
        r#"{"word":"broken","pos":"noun","senses":[}"#.to_string(),
    ];
    let content = lines.join("\n") + "\n";
    let total_bytes = content.len() as u64;

    let mut spec = spec;
    spec.max_words = 2;

    let counter = Arc::new(AtomicU64::new(0));
    let reader = ByteAtATimeReader {
        data: content.as_bytes(),
        pos: 0,
    };
    let counting = CountingReader::new(reader, counter.clone());

    let batches: Arc<Mutex<Vec<Vec<spoor::WordRecord>>>> = Arc::new(Mutex::new(Vec::new()));
    let batches_clone = batches.clone();

    let counter_for_bytes = counter.clone();
    let report = consume_jsonl(
        counting,
        &spec,
        move || counter_for_bytes.load(Ordering::Relaxed),
        |_bytes, _accepted, _skipped| {},
        move |batch| {
            batches_clone.lock().unwrap().push(batch);
        },
    );

    assert_eq!(report.accepted, 2, "should stop after 2 accepted nouns");
    assert_eq!(report.skipped, 0, "verb/broken lines must never be reached");

    let total_records: usize = batches.lock().unwrap().iter().map(|b| b.len()).sum();
    assert_eq!(total_records, 2);

    // Proof of early termination: not all bytes of the input were consumed,
    // because the reader never advanced past line 2 (line 3 onward, incl.
    // the broken JSON line, was never read).
    let bytes_consumed = counter.load(Ordering::Relaxed);
    assert!(
        bytes_consumed < total_bytes,
        "expected early stop, but {} of {} bytes were read",
        bytes_consumed,
        total_bytes
    );
}

/// A `FetchProgress` test double that records calls for later assertions.
struct RecordingProgress {
    errors: Mutex<Vec<(String, String)>>,
    done: Mutex<Vec<FetchReport>>,
}

impl RecordingProgress {
    fn new() -> Self {
        Self {
            errors: Mutex::new(Vec::new()),
            done: Mutex::new(Vec::new()),
        }
    }
}

impl FetchProgress for RecordingProgress {
    fn on_update(&self, _id: &str, _bytes: u64, _accepted: usize, _skipped: usize) {}

    fn on_done(&self, _id: &str, report: &FetchReport) {
        self.done.lock().unwrap().push(report.clone());
    }

    fn on_error(&self, id: &str, msg: &str) {
        self.errors.lock().unwrap().push((id.to_string(), msg.to_string()));
    }
}

#[test]
fn test_fetch_all_reports_error_for_bad_source_without_aborting() {
    let db_file = NamedTempFile::new().unwrap();
    let mut db = Db::open(db_file.path()).unwrap();

    // An invalid URL (no scheme) should fail fast at request time, giving a
    // deterministic, offline-safe error path without hitting the network.
    let bad_spec = SourceSpec {
        id: "bad-source".to_string(),
        backend: Backend::WiktextractJsonl,
        url: "not-a-valid-url".to_string(),
        language: "xx".to_string(),
        system: "test_bad".to_string(),
        max_words: 10,
    };

    let progress = RecordingProgress::new();
    let outcome = fetch_all(&mut db, &[bad_spec], &progress).expect("fetch_all itself must not error");

    assert_eq!(outcome.total_inserted, 0);
    assert_eq!(outcome.reports.len(), 1);
    let report = &outcome.reports[0];
    assert_eq!(report.id, "bad-source");
    assert_eq!(report.accepted, 0);
    assert!(report.error.is_some(), "failing source must carry an error message");

    let errors = progress.errors.lock().unwrap();
    assert_eq!(errors.len(), 1, "on_error must be called exactly once for the bad source");
    assert_eq!(errors[0].0, "bad-source");
}

/// Network smoke test: fetches a real (small) slice of the Latin kaikki.org
/// dictionary and asserts we can stream + parse it end-to-end. Ignored by
/// default since it requires internet access; run with `cargo test -- --ignored`.
#[test]
#[ignore]
fn test_fetch_all_network_smoke_kaikki_la() {
    let db_file = NamedTempFile::new().unwrap();
    let mut db = Db::open(db_file.path()).unwrap();

    let spec = SourceSpec {
        id: "kaikki-la".to_string(),
        backend: Backend::WiktextractJsonl,
        url: "https://kaikki.org/dictionary/Latin/kaikki.org-dictionary-Latin.jsonl".to_string(),
        language: "la".to_string(),
        system: "wiktionary_la".to_string(),
        max_words: 20,
    };

    let progress = RecordingProgress::new();
    let outcome = fetch_all(&mut db, &[spec], &progress).expect("network fetch failed");

    assert_eq!(outcome.reports.len(), 1);
    let report = &outcome.reports[0];
    assert!(report.error.is_none(), "expected no error, got: {:?}", report.error);
    assert_eq!(report.accepted, 20, "expected exactly max_words accepted words");
    assert_eq!(outcome.total_inserted, 20);
}
