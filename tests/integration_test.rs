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
    let first = Generator::new(&cfg, words.clone()).generate_one(&mut rng, &mut used);

    let mut rng = SeededRng::new(7u64);
    let mut used2 = HashSet::new();
    let second = Generator::new(&cfg, words).generate_one(&mut rng, &mut used2);

    assert_eq!(first, second);
    assert!(!first.is_empty());
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
