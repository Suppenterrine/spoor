use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct GeneratorConfig {
    pub prefix_article_probability: f64,
    pub prefix_probability: f64,
    pub suffix_article_probability: f64,
    pub suffix_adjectiv_probability: f64,
    pub suffix_name_probability: f64,
    pub separator: String,
    pub fillword: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct DbConfig {
    pub path: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Config {
    pub generator: GeneratorConfig,
    pub db: DbConfig,
}

impl Config {
    pub fn read(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = fs::read_to_string(&path).with_context(|| {
            format!(
                "Failed to read config file: {}",
                path.as_ref().display()
            )
        })?;
        let cfg: Config = toml::from_str(&content).with_context(|| {
            format!(
                "Failed to parse config: {}",
                path.as_ref().display()
            )
        })?;
        Ok(cfg)
    }
}
