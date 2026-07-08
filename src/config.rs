use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

/// Default database path: $DATA_DIR/spoor/words.db (or data/words.db if $DATA_DIR is unavailable)
pub fn default_db_path() -> PathBuf {
    dirs::data_dir()
        .map(|d| d.join("spoor").join("words.db"))
        .unwrap_or_else(|| PathBuf::from("data/words.db"))
}

#[derive(Debug, Deserialize, Clone)]
pub struct GeneratorConfig {
    pub prefix_article_probability: f64,
    pub prefix_probability: f64,
    pub suffix_article_probability: f64,
    pub suffix_adjectiv_probability: f64,
    pub suffix_name_probability: f64,
    pub separator: String,
    pub fillword: String,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            prefix_article_probability: 0.2,
            prefix_probability: 0.8,
            suffix_article_probability: 0.3,
            suffix_adjectiv_probability: 0.5,
            suffix_name_probability: 0.5,
            separator: " ".to_string(),
            fillword: "of".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct DbConfig {
    #[serde(default = "default_db_path")]
    pub path: PathBuf,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub generator: GeneratorConfig,
    #[serde(default)]
    pub db: DbConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            generator: GeneratorConfig::default(),
            db: DbConfig::default(),
        }
    }
}

impl Config {
    /// Load config from path. If file doesn't exist and explicit=false, return defaults.
    /// If file doesn't exist and explicit=true, return error.
    pub fn load(path: &str, explicit: bool) -> anyhow::Result<Self> {
        match fs::read_to_string(path) {
            Ok(content) => {
                let mut cfg: Config = toml::from_str(&content).with_context(|| {
                    format!("Failed to parse config: {}", path)
                })?;
                // Ensure db.path is set even if [db] section is missing or path is empty
                if cfg.db.path.as_os_str().is_empty() {
                    cfg.db.path = default_db_path();
                }
                Ok(cfg)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if explicit {
                    Err(anyhow::anyhow!(
                        "Config file not found: {} (explicitly specified)",
                        path
                    ))
                } else {
                    Ok(Self::default())
                }
            }
            Err(e) => Err(e).with_context(|| format!("Failed to read config file: {}", path)),
        }
    }

    /// Legacy method for backward compatibility
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
