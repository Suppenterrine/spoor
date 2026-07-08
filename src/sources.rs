use std::path::Path;
use serde::{Deserialize, Serialize};
use anyhow::Context;

/// Supported backend types for word source parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Backend {
    #[serde(rename = "wiktextract-jsonl")]
    WiktextractJsonl,
}

impl Backend {
    /// Return all supported backend type names for error messaging.
    fn supported() -> &'static [&'static str] {
        &["wiktextract-jsonl"]
    }
}

/// Configuration for a single word source.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceSpec {
    pub id: String,
    pub backend: Backend,
    pub url: String,
    pub language: String,
    pub system: String,
    #[serde(default = "default_max_words")]
    pub max_words: usize,
}

/// Default max_words value.
fn default_max_words() -> usize {
    500
}

/// Root configuration structure for sources.yaml.
#[derive(Debug, Deserialize)]
pub struct SourcesConfig {
    pub sources: Vec<SourceSpec>,
}

/// Load and parse sources.yaml file.
/// Returns an error with context-rich message if backend is unknown.
pub fn load_sources(path: impl AsRef<Path>) -> anyhow::Result<SourcesConfig> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read sources file at {:?}", path))?;

    let config: SourcesConfig = serde_yaml::from_str(&content)
        .with_context(|| {
            format!(
                "failed to parse YAML from {:?}. Ensure 'backend' is one of: {}",
                path,
                Backend::supported().join(", ")
            )
        })?;

    // Validate all backends are recognized
    for source in &config.sources {
        if source.backend != Backend::WiktextractJsonl {
            anyhow::bail!(
                "unknown backend '{}' in source '{}'. Supported backends: {}",
                format!("{:?}", source.backend).to_lowercase(),
                source.id,
                Backend::supported().join(", ")
            );
        }
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_load_sources_valid() {
        let yaml_content = r#"
# Wortquellen fuer 'name-generator db fetch'.
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
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml_content.as_bytes()).unwrap();
        file.flush().unwrap();

        let config = load_sources(file.path()).expect("failed to load valid YAML");
        assert_eq!(config.sources.len(), 2);
        assert_eq!(config.sources[0].id, "kaikki-de");
        assert_eq!(config.sources[0].backend, Backend::WiktextractJsonl);
        assert_eq!(config.sources[0].language, "de");
        assert_eq!(config.sources[0].max_words, 250);
        assert_eq!(config.sources[1].id, "kaikki-en");
        assert_eq!(config.sources[1].max_words, 500); // default
    }

    #[test]
    fn test_load_sources_unknown_backend() {
        let yaml_content = r#"
sources:
  - id: bad-source
    backend: unknown-backend
    url: https://example.org/words.jsonl
    language: xx
    system: bad_system
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml_content.as_bytes()).unwrap();
        file.flush().unwrap();

        let result = load_sources(file.path());
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        // Should contain info about supported backends
        assert!(err_msg.contains("wiktextract-jsonl"));
    }
}
