use std::collections::HashSet;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

use spoor::db::Db;
use spoor::config::Config;
use spoor::generator::{Generator, SeededRng, WordLists};
use spoor::SEED_WORDS_CSV;

#[test]
fn csv_import_db_insert_and_deterministic_generate() {
    let dir = TempDir::new().unwrap();
    let csv_path = dir.path().join("words.csv");
    let db_path = dir.path().join("words.db");

    fs::write(
        &csv_path,
        "word,language,word_class,system,tags,seed_weight,source\nalpha,en,noun,nature,test,1.0,wiki\nbeta,en,proper,nature,boss,1.2,curated\n",
    )
    .unwrap();

    let records = read_csv(&csv_path);
    assert_eq!(records.len(), 2);

    let mut db = Db::open(&db_path).unwrap();
    db.insert_words(&records).unwrap();
    assert_eq!(db.stats().unwrap().total, 2);

    let words = WordLists {
        prefixes: vec![],
        words: vec!["alpha".into(), "beta".into()],
        suffix_adjs: vec![],
        suffix_names: vec![],
    };

    let cfg = Config {
        generator: spoor::config::GeneratorConfig {
            prefix_article_probability: 0.0,
            prefix_probability: 0.0,
            suffix_article_probability: 0.0,
            suffix_adjectiv_probability: 0.0,
            suffix_name_probability: 0.0,
            separator: " ".into(),
            fillword: "of".into(),
        },
        db: spoor::config::DbConfig {
            path: db_path,
        },
    };

    let mut rng = SeededRng::new(7u64);
    let mut used = HashSet::new();
    let first = Generator::new(&cfg, words.clone()).generate_unique(&mut rng, &mut used, 100);

    let mut rng = SeededRng::new(7u64);
    let mut used2 = HashSet::new();
    let second = Generator::new(&cfg, words).generate_unique(&mut rng, &mut used2, 100);

    assert_eq!(first, second);
    assert!(!first.unwrap().is_empty());
}

#[test]
fn template_determinism() {
    let words = WordLists {
        prefixes: vec![],
        words: vec!["alpha".into(), "beta".into()],
        suffix_adjs: vec![],
        suffix_names: vec!["dawn".into(), "dusk".into()],
    };

    let cfg = Config {
        generator: spoor::config::GeneratorConfig {
            prefix_article_probability: 0.0,
            prefix_probability: 0.0,
            suffix_article_probability: 0.0,
            suffix_adjectiv_probability: 0.0,
            suffix_name_probability: 0.0,
            separator: " ".into(),
            fillword: "of".into(),
        },
        db: spoor::config::DbConfig {
            path: ":memory:".into(),
        },
    };

    let gen = Generator::with_template(&cfg, words.clone(), "The {word} of {suffix}").unwrap();

    let mut rng = SeededRng::new(42u64);
    let mut used = HashSet::new();
    let first = gen.generate_unique(&mut rng, &mut used, 100).unwrap();

    let mut rng = SeededRng::new(42u64);
    let mut used2 = HashSet::new();
    let second = gen.generate_unique(&mut rng, &mut used2, 100).unwrap();

    assert_eq!(first, second);
    assert!(first.contains("The "));
    assert!(first.contains(" of "));
}

#[test]
fn template_empty_slot_leaves_no_stray_whitespace() {
    let words = WordLists {
        prefixes: vec![],
        words: vec!["alpha".into()],
        suffix_adjs: vec![],
        suffix_names: vec![],
    };

    let cfg = Config {
        generator: spoor::config::GeneratorConfig {
            prefix_article_probability: 0.0,
            prefix_probability: 0.0,
            suffix_article_probability: 0.0,
            suffix_adjectiv_probability: 0.0,
            suffix_name_probability: 0.0,
            separator: " ".into(),
            fillword: "of".into(),
        },
        db: spoor::config::DbConfig {
            path: ":memory:".into(),
        },
    };

    let gen = Generator::with_template(&cfg, words, "{prefix} {word} of {suffix}").unwrap();

    let mut rng = SeededRng::new(5u64);
    let mut used = HashSet::new();
    let result = gen.generate_unique(&mut rng, &mut used, 100).unwrap();

    assert_eq!(result, "alpha of");
    assert_eq!(result, result.trim());
}

#[test]
fn no_word_corruption() {
    let words = WordLists {
        prefixes: vec![],
        words: vec!["Profound".into()],
        suffix_adjs: vec!["luminous".into()],
        suffix_names: vec!["Dawn".into()],
    };

    let cfg = Config {
        generator: spoor::config::GeneratorConfig {
            prefix_article_probability: 0.0,
            prefix_probability: 0.0,
            suffix_article_probability: 1.0,
            suffix_adjectiv_probability: 1.0,
            suffix_name_probability: 1.0,
            separator: " ".into(),
            fillword: "of".into(),
        },
        db: spoor::config::DbConfig {
            path: ":memory:".into(),
        },
    };

    let gen = Generator::new(&cfg, words);

    let mut rng = SeededRng::new(1u64);
    let mut used = HashSet::new();
    let result = gen.generate_unique(&mut rng, &mut used, 100).unwrap();

    assert_eq!(result, "Profound of the luminous Dawn");
}

#[test]
fn exhaustion_safety() {
    let words = WordLists {
        prefixes: vec![],
        words: vec!["alpha".into(), "beta".into()],
        suffix_adjs: vec![],
        suffix_names: vec![],
    };

    let cfg = Config {
        generator: spoor::config::GeneratorConfig {
            prefix_article_probability: 0.0,
            prefix_probability: 0.0,
            suffix_article_probability: 0.0,
            suffix_adjectiv_probability: 0.0,
            suffix_name_probability: 0.0,
            separator: " ".into(),
            fillword: "of".into(),
        },
        db: spoor::config::DbConfig {
            path: ":memory:".into(),
        },
    };

    let gen = Generator::new(&cfg, words);

    let mut rng = SeededRng::new(100u64);
    let mut used = HashSet::new();

    let first = gen.generate_unique(&mut rng, &mut used, 100);
    assert!(first.is_some());
    let first_val = first.unwrap();

    let second = gen.generate_unique(&mut rng, &mut used, 100);
    assert!(second.is_some());
    let second_val = second.unwrap();

    assert_ne!(first_val, second_val);

    let third = gen.generate_unique(&mut rng, &mut used, 100);
    assert!(third.is_none());
}

fn read_csv(path: &std::path::Path) -> Vec<spoor::db::WordRecord> {
    let mut reader = csv::Reader::from_path(path).unwrap();
    let mut records = Vec::new();
    for result in reader.records() {
        let record = result.unwrap();
        records.push(spoor::db::WordRecord::parse_csv_record(&record).unwrap());
    }
    records
}

#[test]
fn csv_with_etymology_roundtrip() {
    let dir = TempDir::new().unwrap();
    let csv_path = dir.path().join("words.csv");
    let db_path = dir.path().join("words.db");

    // Create 9-column CSV with etymology and origin_lang; one row with empty values
    fs::write(
        &csv_path,
        "word,language,word_class,system,tags,seed_weight,source,etymology,origin_lang\n\
         alpha,en,noun,nature,test,1.0,wiki,from Greek alpha,Greek\n\
         beta,en,proper,nature,boss,1.2,curated,,\n",
    )
    .unwrap();

    let records = read_csv(&csv_path);
    assert_eq!(records.len(), 2);

    // Check first record has etymology and origin_lang
    assert_eq!(records[0].etymology, Some("from Greek alpha".to_string()));
    assert_eq!(records[0].origin_lang, Some("Greek".to_string()));

    // Check second record has None for empty fields
    assert_eq!(records[1].etymology, None);
    assert_eq!(records[1].origin_lang, None);

    // Insert into database and verify roundtrip
    let mut db = Db::open(&db_path).unwrap();
    db.insert_words(&records).unwrap();

    let retrieved = db.all_records(None).unwrap();
    assert_eq!(retrieved.len(), 2);

    assert_eq!(retrieved[0].word, "alpha");
    assert_eq!(retrieved[0].etymology, Some("from Greek alpha".to_string()));
    assert_eq!(retrieved[0].origin_lang, Some("Greek".to_string()));

    assert_eq!(retrieved[1].word, "beta");
    assert_eq!(retrieved[1].etymology, None);
    assert_eq!(retrieved[1].origin_lang, None);
}

#[test]
fn schema_migration_adds_columns() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("words.db");

    // Create an old-style database with only the original 8 columns
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE words (
                id TEXT PRIMARY KEY,
                word TEXT NOT NULL,
                word_class TEXT,
                language TEXT,
                system TEXT,
                tags TEXT,
                seed_weight REAL DEFAULT 1.0,
                source TEXT
            )",
            [],
        )
        .unwrap();

        // Insert a test row
        conn.execute(
            "INSERT INTO words (id, word, word_class, language, system, tags, seed_weight, source) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params!["en_oak", "oak", "noun", "en", "nature", "tree,strength", 1.0, "wiki"],
        )
        .unwrap();
    }

    // Now open with Db::open, which should migrate the schema
    let db = Db::open(&db_path).unwrap();

    // Verify that all_records works and returns the row with etymology and origin_lang as None
    let records = db.all_records(None).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].word, "oak");
    assert_eq!(records[0].etymology, None);
    assert_eq!(records[0].origin_lang, None);

    // Verify the columns were added by checking PRAGMA table_info
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut stmt = conn
        .prepare("PRAGMA table_info(words)")
        .unwrap();
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get(1))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(columns.contains(&"etymology".to_string()));
    assert!(columns.contains(&"origin_lang".to_string()));
}

#[test]
fn db_stats_and_class_query() {
    let dir = TempDir::new().unwrap();
    let csv_path = dir.path().join("words.csv");
    let db_path = dir.path().join("words.db");

    // Create CSV with two languages, two systems, and noun/prefix/suffix classes
    fs::write(
        &csv_path,
        "word,language,word_class,system,tags,seed_weight,source\n\
         oak,en,noun,nature,\"tree,strong\",1.0,wiki\n\
         silent,en,prefix,nature,quiet,1.0,curated\n\
         glory,en,suffix,nature,honor,1.0,curated\n\
         eiche,de,noun,craft,wood,1.0,wiki\n\
         golden,de,prefix,craft,shiny,1.0,curated\n\
         macht,de,suffix,craft,power,1.0,curated\n",
    )
    .unwrap();

    let records = read_csv(&csv_path);
    assert_eq!(records.len(), 6);

    let mut db = Db::open(&db_path).unwrap();
    db.insert_words(&records).unwrap();

    // Test stats
    let stats = db.stats().unwrap();
    assert_eq!(stats.total, 6);

    // Check by_language counts
    let en_count = stats.by_language.iter().find(|(lang, _)| lang == "en").map(|(_, cnt)| cnt);
    let de_count = stats.by_language.iter().find(|(lang, _)| lang == "de").map(|(_, cnt)| cnt);
    assert_eq!(en_count, Some(&3));
    assert_eq!(de_count, Some(&3));

    // Check by_system counts
    let nature_count = stats.by_system.iter().find(|(sys, _)| sys == "nature").map(|(_, cnt)| cnt);
    let craft_count = stats.by_system.iter().find(|(sys, _)| sys == "craft").map(|(_, cnt)| cnt);
    assert_eq!(nature_count, Some(&3));
    assert_eq!(craft_count, Some(&3));

    // Test words_by_class with system filter
    let nature_words = db.words_by_class(Some(&["nature".to_string()])).unwrap();
    assert_eq!(nature_words.len(), 3);
    assert!(nature_words.iter().any(|(w, _)| w == "oak"));
    assert!(nature_words.iter().any(|(w, _)| w == "silent"));
    assert!(nature_words.iter().any(|(w, _)| w == "glory"));
    assert!(!nature_words.iter().any(|(w, _)| w == "eiche"));
    assert!(!nature_words.iter().any(|(w, _)| w == "golden"));

    // Test words_by_class without filter (all words)
    let all_words = db.words_by_class(None).unwrap();
    assert_eq!(all_words.len(), 6);
}

#[test]
fn find_systems_filter_in_sql() {
    let dir = TempDir::new().unwrap();
    let csv_path = dir.path().join("words.csv");
    let db_path = dir.path().join("words.db");

    // Create CSV with two systems
    fs::write(
        &csv_path,
        "word,language,word_class,system,tags,seed_weight,source\n\
         oak,en,noun,nature,tree,1.0,wiki\n\
         birch,en,noun,nature,tree,1.0,wiki\n\
         zeus,la,proper,myth_greek,sky,1.2,curated\n\
         hades,la,proper,myth_greek,underworld,1.2,curated\n",
    )
    .unwrap();

    let records = read_csv(&csv_path);
    let mut db = Db::open(&db_path).unwrap();
    db.insert_words(&records).unwrap();

    // Test all_records with nature system filter
    let nature_records = db.all_records(Some(&["nature".to_string()])).unwrap();
    assert_eq!(nature_records.len(), 2);
    assert!(nature_records.iter().any(|r| r.word == "oak"));
    assert!(nature_records.iter().any(|r| r.word == "birch"));
    assert!(!nature_records.iter().any(|r| r.word == "zeus"));
    assert!(!nature_records.iter().any(|r| r.word == "hades"));

    // Test all_records without filter
    let all_records = db.all_records(None).unwrap();
    assert_eq!(all_records.len(), 4);
}

#[test]
fn import_reports_unknown_classes() {
    let dir = TempDir::new().unwrap();
    let csv_path = dir.path().join("words.csv");
    let db_path = dir.path().join("words.db");

    // Create CSV with one recognized and one unrecognized word_class
    fs::write(
        &csv_path,
        "word,language,word_class,system,tags,seed_weight,source\n\
         oak,en,noun,nature,tree,1.0,wiki\n\
         thunder,en,verb,nature,sound,1.0,wiki\n",
    )
    .unwrap();

    let mut db = Db::open(&db_path).unwrap();
    let report = db.import_csv(&csv_path).unwrap();

    assert_eq!(report.imported, 2);
    assert_eq!(report.unknown_class, 1);
}

#[test]
fn import_csv_streams_and_counts() {
    let dir = TempDir::new().unwrap();
    let csv_path = dir.path().join("words.csv");
    let db_path = dir.path().join("words.db");

    // Create a small CSV
    fs::write(
        &csv_path,
        "word,language,word_class,system,tags,seed_weight,source,etymology,origin_lang\n\
         alpha,en,noun,nature,test,1.0,wiki,from Greek alpha,Greek\n\
         beta,en,proper,nature,boss,1.2,curated,,\n\
         gamma,en,prefix,nature,letter,1.0,curated,,\n",
    )
    .unwrap();

    let mut db = Db::open(&db_path).unwrap();
    let report = db.import_csv(&csv_path).unwrap();

    assert_eq!(report.imported, 3);
    assert_eq!(report.unknown_class, 0);

    // Verify the rows are queryable
    let all = db.all_records(None).unwrap();
    assert_eq!(all.len(), 3);
    assert!(all.iter().any(|r| r.word == "alpha"));
    assert!(all.iter().any(|r| r.word == "beta"));
    assert!(all.iter().any(|r| r.word == "gamma"));

    // Verify etymology was preserved
    let alpha = all.iter().find(|r| r.word == "alpha").unwrap();
    assert_eq!(alpha.etymology, Some("from Greek alpha".to_string()));
}

#[test]
fn import_csv_reader_from_in_memory_string() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("words.db");

    let csv_data = "word,language,word_class,system,tags,seed_weight,source,etymology,origin_lang
alpha,en,noun,nature,test,1.0,wiki,from Greek alpha,Greek
beta,en,proper,nature,boss,1.2,curated,,
gamma,en,prefix,nature,letter,1.0,curated,,";

    let mut db = Db::open(&db_path).unwrap();
    let report = db.import_csv_reader(csv_data.as_bytes()).unwrap();

    assert_eq!(report.imported, 3);
    assert_eq!(report.unknown_class, 0);

    // Verify the rows are queryable
    let all = db.all_records(None).unwrap();
    assert_eq!(all.len(), 3);
    assert!(all.iter().any(|r| r.word == "alpha"));
    assert!(all.iter().any(|r| r.word == "beta"));
    assert!(all.iter().any(|r| r.word == "gamma"));
}

#[test]
fn bootstrap_with_embedded_seed_data() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("words.db");

    // First call: DB doesn't exist, should bootstrap
    {
        let mut db = Db::open(&db_path).unwrap();
        let report = db.import_csv_reader(SEED_WORDS_CSV.as_bytes()).unwrap();

        // Should have imported 77 words (header + 77 data rows)
        assert_eq!(report.imported, 77);
    }

    // Verify the DB was created and seeded
    assert!(db_path.exists());
    let db = Db::open(&db_path).unwrap();
    let stats = db.stats().unwrap();
    assert_eq!(stats.total, 77);

    // Second call: DB exists, should not re-seed (this is tested by not calling import again)
    let db2 = Db::open(&db_path).unwrap();
    let stats2 = db2.stats().unwrap();
    assert_eq!(stats2.total, 77); // Still 77, not 154
}

#[test]
fn seed_words_csv_has_correct_header() {
    // Verify the embedded CSV starts with the expected header
    let lines: Vec<&str> = SEED_WORDS_CSV.lines().collect();
    assert!(!lines.is_empty());

    let header = lines[0];
    assert!(header.contains("word"));
    assert!(header.contains("language"));
    assert!(header.contains("word_class"));
    assert!(header.contains("system"));
    assert!(header.contains("tags"));
    assert!(header.contains("seed_weight"));
    assert!(header.contains("source"));
    assert!(header.contains("etymology"));
    assert!(header.contains("origin_lang"));
}

#[test]
fn bare_invocation_shows_status_screen() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("words.db");
    let config_path = dir.path().join("config.toml");

    // Create a minimal config pointing to the test database
    let config_content = format!(
        "[db]\npath = \"{}\"",
        db_path.display().to_string().replace('\\', "\\\\")
    );
    fs::write(&config_path, config_content).unwrap();

    // Seed the database
    {
        let mut db = Db::open(&db_path).unwrap();
        db.import_csv_reader(SEED_WORDS_CSV.as_bytes()).unwrap();
    }

    // Run `spoor` with no args using the binary
    let output = Command::new(env!("CARGO_BIN_EXE_spoor"))
        .arg("--config")
        .arg(&config_path)
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to execute spoor binary");

    // Check exit code is 0
    assert!(output.status.success(), "Exit code should be 0, got: {:?}", output.status);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify key strings from the status screen are present
    assert!(stdout.contains("spoor"), "Should contain version info");
    assert!(stdout.contains("folge der Bedeutung zum Namen"), "Should have German tagline");
    assert!(stdout.contains("Wortbestand"), "Should show word inventory");
    assert!(stdout.contains("WOMIT MOECHTEST DU STARTEN?"), "Should have German prompt");
    assert!(stdout.contains("spoor find"), "Should mention find command");
    assert!(stdout.contains("spoor gen"), "Should mention gen command");
    assert!(stdout.contains("spoor db fetch"), "Should mention db fetch command");
}
