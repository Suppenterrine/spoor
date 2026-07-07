use std::fs;
use tempfile::TempDir;

use name_generator::db::Db;
use name_generator::config::Config;
use name_generator::generator::{Generator, SeededRng, WordLists};

#[test]
fn csv_roundtrip_and_rng_determinism() {
    let dir = TempDir::new().unwrap();
    let csv_path = dir.path().join("words.csv");
    let db_path = dir.path().join("words.db");

    fs::write(
        &csv_path,
        "word,language,word_class,system,tags,seed_weight,source\nalpha,en,noun,nature,test,1.0,wiki\nbeta,en,proper,nature,boss,1.2,curated\ngamma,en,adj,nature,glow,1.0,wiki\ndelta,en,suffix,nature,stone,1.1,wiki\nepsilon,en,prefix,nature,crown,1.0,wiki\n",
    )
    .unwrap();

    let records = read_csv(&csv_path);
    assert_eq!(records.len(), 5);
    let mut db = Db::open(&db_path).unwrap();
    db.insert_words(&records).unwrap();
    assert_eq!(db.total().unwrap(), 5);

    let mut words = WordLists { prefixes: vec!["epsilon".into()], words: vec!["alpha".into(), "beta".into()], suffix_adjs: vec!["gamma".into()], suffix_names: vec!["delta".into()] };

    let cfg = Config { generator: name_generator::config::GeneratorConfig { prefix_article_probability: 1.0, prefix_probability: 1.0, suffix_article_probability: 0.0, suffix_adjectiv_probability: 1.0, suffix_name_probability: 1.0, separator: " ".into(), fillword: "of".into() }, db: name_generator::config::DbConfig { path: db_path.display().to_string() } };

    let mut rng = SeededRng::new(7u64);
    let mut used = std::collections::HashSet::new();
    let gen = Generator::new(&cfg, words.clone());
    let first = gen.generate_one(&mut rng, &mut used);

    let mut rng = SeededRng::new(7u64);
    let mut used2 = std::collections::HashSet::new();
    let gen2 = Generator::new(&cfg, words);
    let second = gen2.generate_one(&mut rng, &mut used2);

    assert_eq!(first, second);
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
