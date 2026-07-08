use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

use name_generator::db::Db;
use name_generator::config::Config;
use name_generator::generator::{Generator, SeededRng, WordLists};

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
    assert_eq!(db.total().unwrap(), 2);

    let words = WordLists {
        prefixes: vec![],
        words: vec!["alpha".into(), "beta".into()],
        suffix_adjs: vec![],
        suffix_names: vec![],
    };

    let cfg = Config {
        generator: name_generator::config::GeneratorConfig {
            prefix_article_probability: 0.0,
            prefix_probability: 0.0,
            suffix_article_probability: 0.0,
            suffix_adjectiv_probability: 0.0,
            suffix_name_probability: 0.0,
            separator: " ".into(),
            fillword: "of".into(),
        },
        db: name_generator::config::DbConfig {
            path: db_path.display().to_string(),
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
        generator: name_generator::config::GeneratorConfig {
            prefix_article_probability: 0.0,
            prefix_probability: 0.0,
            suffix_article_probability: 0.0,
            suffix_adjectiv_probability: 0.0,
            suffix_name_probability: 0.0,
            separator: " ".into(),
            fillword: "of".into(),
        },
        db: name_generator::config::DbConfig {
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
        generator: name_generator::config::GeneratorConfig {
            prefix_article_probability: 0.0,
            prefix_probability: 0.0,
            suffix_article_probability: 0.0,
            suffix_adjectiv_probability: 0.0,
            suffix_name_probability: 0.0,
            separator: " ".into(),
            fillword: "of".into(),
        },
        db: name_generator::config::DbConfig {
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
        generator: name_generator::config::GeneratorConfig {
            prefix_article_probability: 0.0,
            prefix_probability: 0.0,
            suffix_article_probability: 1.0,
            suffix_adjectiv_probability: 1.0,
            suffix_name_probability: 1.0,
            separator: " ".into(),
            fillword: "of".into(),
        },
        db: name_generator::config::DbConfig {
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
        generator: name_generator::config::GeneratorConfig {
            prefix_article_probability: 0.0,
            prefix_probability: 0.0,
            suffix_article_probability: 0.0,
            suffix_adjectiv_probability: 0.0,
            suffix_name_probability: 0.0,
            separator: " ".into(),
            fillword: "of".into(),
        },
        db: name_generator::config::DbConfig {
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

fn read_csv(path: &std::path::Path) -> Vec<name_generator::db::WordRecord> {
    let mut reader = csv::Reader::from_path(path).unwrap();
    let mut records = Vec::new();
    for result in reader.records() {
        let record = result.unwrap();
        records.push(name_generator::db::WordRecord::parse_csv_record(&record).unwrap());
    }
    records
}
