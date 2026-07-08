use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use spoor::config::{Config, GeneratorConfig, DbConfig, default_db_path};

#[test]
fn config_load_missing_file_explicit_false_returns_defaults() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("nonexistent.toml");

    let cfg = Config::load(config_path.to_str().unwrap(), false).unwrap();

    // Check defaults
    assert_eq!(cfg.generator.prefix_article_probability, 0.2);
    assert_eq!(cfg.generator.prefix_probability, 0.8);
    assert_eq!(cfg.generator.suffix_article_probability, 0.3);
    assert_eq!(cfg.generator.suffix_adjectiv_probability, 0.5);
    assert_eq!(cfg.generator.suffix_name_probability, 0.5);
    assert_eq!(cfg.generator.separator, " ");
    assert_eq!(cfg.generator.fillword, "of");
}

#[test]
fn config_load_missing_file_explicit_true_returns_error() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("nonexistent.toml");

    let result = Config::load(config_path.to_str().unwrap(), true);

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("explicitly specified"));
}

#[test]
fn config_load_existing_file_parses_correctly() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.toml");

    fs::write(
        &config_path,
        "[generator]\n\
prefix_article_probability = 0.25\n\
prefix_probability = 0.75\n\
suffix_article_probability = 0.35\n\
suffix_adjectiv_probability = 0.55\n\
suffix_name_probability = 0.45\n\
separator = \"-\"\n\
fillword = \"and\"\n\
\n\
[db]\n\
path = \"custom/words.db\"\n",
    )
    .unwrap();

    let cfg = Config::load(config_path.to_str().unwrap(), false).unwrap();

    assert_eq!(cfg.generator.prefix_article_probability, 0.25);
    assert_eq!(cfg.generator.prefix_probability, 0.75);
    assert_eq!(cfg.generator.suffix_article_probability, 0.35);
    assert_eq!(cfg.generator.suffix_adjectiv_probability, 0.55);
    assert_eq!(cfg.generator.suffix_name_probability, 0.45);
    assert_eq!(cfg.generator.separator, "-");
    assert_eq!(cfg.generator.fillword, "and");
    assert_eq!(cfg.db.path, PathBuf::from("custom/words.db"));
}

#[test]
fn config_load_missing_db_section_uses_default() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.toml");

    fs::write(
        &config_path,
        "[generator]\n\
prefix_article_probability = 0.2\n\
prefix_probability = 0.8\n\
suffix_article_probability = 0.3\n\
suffix_adjectiv_probability = 0.5\n\
suffix_name_probability = 0.5\n\
separator = \" \"\n\
fillword = \"of\"\n",
    )
    .unwrap();

    let cfg = Config::load(config_path.to_str().unwrap(), false).unwrap();

    // db.path should be default_db_path() or something similar
    assert!(!cfg.db.path.as_os_str().is_empty());
}

#[test]
fn config_load_empty_db_path_uses_default() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.toml");

    fs::write(
        &config_path,
        "[generator]\n\
prefix_article_probability = 0.2\n\
prefix_probability = 0.8\n\
suffix_article_probability = 0.3\n\
suffix_adjectiv_probability = 0.5\n\
suffix_name_probability = 0.5\n\
separator = \" \"\n\
fillword = \"of\"\n\
\n\
[db]\n\
path = \"\"\n",
    )
    .unwrap();

    let cfg = Config::load(config_path.to_str().unwrap(), false).unwrap();

    // Empty path should be replaced with default
    assert!(!cfg.db.path.as_os_str().is_empty());
}

#[test]
fn generator_config_default_has_correct_values() {
    let cfg = GeneratorConfig::default();

    assert_eq!(cfg.prefix_article_probability, 0.2);
    assert_eq!(cfg.prefix_probability, 0.8);
    assert_eq!(cfg.suffix_article_probability, 0.3);
    assert_eq!(cfg.suffix_adjectiv_probability, 0.5);
    assert_eq!(cfg.suffix_name_probability, 0.5);
    assert_eq!(cfg.separator, " ");
    assert_eq!(cfg.fillword, "of");
}

#[test]
fn db_config_default_uses_default_db_path() {
    let cfg = DbConfig::default();

    assert!(!cfg.path.as_os_str().is_empty());
    // Should point to either user data dir or data/words.db
    let default_path = default_db_path();
    assert_eq!(cfg.path, default_path);
}

#[test]
fn config_default_combines_defaults() {
    let cfg = Config::default();

    // Check generator defaults
    assert_eq!(cfg.generator.prefix_article_probability, 0.2);
    assert_eq!(cfg.generator.fillword, "of");

    // Check db defaults
    assert!(!cfg.db.path.as_os_str().is_empty());
}
