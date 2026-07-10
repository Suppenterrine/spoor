use std::path::Path;
use serde::{Deserialize, Serialize};
use anyhow::Context;

/// Supported backend types for word source parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Backend {
    #[serde(rename = "wiktextract-jsonl")]
    WiktextractJsonl,
    /// Whole-body JSON array of mythological figures (greek-mythology-data format)
    #[serde(rename = "mythology-json")]
    MythologyJson,
}

impl Backend {
    /// Return all supported backend type names for error messaging.
    fn supported() -> &'static [&'static str] {
        &["wiktextract-jsonl", "mythology-json"]
    }
}

/// Supported backend types for query expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ExpansionBackend {
    #[serde(rename = "datamuse-ml")]
    DatamuseMl,
}

impl ExpansionBackend {
    /// Return all supported expansion backend type names for error messaging.
    fn supported() -> &'static [&'static str] {
        &["datamuse-ml"]
    }
}

/// Configuration for semantic query expansion.
#[derive(Debug, Clone, Deserialize)]
pub struct QueryExpansionSpec {
    pub backend: ExpansionBackend,
    pub url: String,
    #[serde(default = "default_max_candidates")]
    pub max_candidates: usize,
}

/// Default max_candidates value for query expansion.
fn default_max_candidates() -> usize {
    10
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
    /// Word classes to skip during parsing (e.g., ["proper"]).
    /// Matched classes: proper, noun, adj
    #[serde(default)]
    pub skip_classes: Vec<String>,
}

/// Default max_words value.
fn default_max_words() -> usize {
    500
}

/// Root configuration structure for sources.yaml.
#[derive(Debug, Deserialize)]
pub struct SourcesConfig {
    pub sources: Vec<SourceSpec>,
    #[serde(default)]
    pub query_expansion: Option<QueryExpansionSpec>,
}

/// The repository's sources.yaml, embedded at compile time so 'spoor db fetch'
/// and '--online' work from any working directory, not just inside the repo
/// checkout (mirrors the embedded seed words in SEED_WORDS_CSV).
pub const EMBEDDED_SOURCES_YAML: &str = include_str!("../sources.yaml");

/// Parse sources.yaml content already read into memory.
/// Returns an error with context-rich message if backend is unknown.
fn parse_sources(content: &str) -> anyhow::Result<SourcesConfig> {
    let config: SourcesConfig = serde_yaml::from_str(content)
        .with_context(|| {
            format!(
                "failed to parse sources YAML. Ensure 'backend' is one of: {}",
                Backend::supported().join(", ")
            )
        })?;

    // Backend values are validated by serde at parse time (unknown values
    // fail deserialization with the context message above); every enum
    // variant has an implementation in fetch::fetch_all.

    // Validate query_expansion backend if present
    if let Some(ref expansion) = config.query_expansion {
        if expansion.backend != ExpansionBackend::DatamuseMl {
            anyhow::bail!(
                "unknown expansion backend. Supported: {}",
                ExpansionBackend::supported().join(", ")
            );
        }
    }

    Ok(config)
}

/// Load and parse sources.yaml from an explicit path.
pub fn load_sources(path: impl AsRef<Path>) -> anyhow::Result<SourcesConfig> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read sources file at {:?}", path))?;
    parse_sources(&content)
}

/// Load sources.yaml, preferring a local file at `path` when present, and
/// falling back to the copy embedded in the binary otherwise. This is what
/// lets 'spoor' find its default sources regardless of the current working
/// directory - only an explicit, unreadable custom path is treated as an error.
pub fn load_sources_or_embedded(path: impl AsRef<Path>) -> anyhow::Result<SourcesConfig> {
    let path = path.as_ref();
    match std::fs::read_to_string(path) {
        Ok(content) => parse_sources(&content),
        Err(_) => parse_sources(EMBEDDED_SOURCES_YAML),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_load_sources_valid() {
        let yaml_content = r#"
# Wortquellen fuer 'spoor db fetch'.
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
        assert_eq!(config.sources[0].skip_classes.is_empty(), true);
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

    #[test]
    fn test_load_sources_with_skip_classes() {
        let yaml_content = r#"
sources:
  - id: kaikki-grc
    backend: wiktextract-jsonl
    url: https://example.org/grc.jsonl
    language: grc
    system: wiktionary_grc
    skip_classes: [proper]
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml_content.as_bytes()).unwrap();
        file.flush().unwrap();

        let config = load_sources(file.path()).expect("failed to load valid YAML");
        assert_eq!(config.sources.len(), 1);
        assert_eq!(config.sources[0].skip_classes.len(), 1);
        assert_eq!(config.sources[0].skip_classes[0], "proper");
    }

    #[test]
    fn test_load_sources_with_query_expansion() {
        let yaml_content = r#"
sources:
  - id: kaikki-de
    backend: wiktextract-jsonl
    url: https://example.org/de.jsonl
    language: de
    system: wiktionary_de

query_expansion:
  backend: datamuse-ml
  url: https://api.datamuse.com/words
  max_candidates: 10
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml_content.as_bytes()).unwrap();
        file.flush().unwrap();

        let config = load_sources(file.path()).expect("failed to load valid YAML");
        assert!(config.query_expansion.is_some());
        let expansion = config.query_expansion.unwrap();
        assert_eq!(expansion.backend, ExpansionBackend::DatamuseMl);
        assert_eq!(expansion.url, "https://api.datamuse.com/words");
        assert_eq!(expansion.max_candidates, 10);
    }

    #[test]
    fn test_load_sources_unknown_expansion_backend() {
        let yaml_content = r#"
sources:
  - id: kaikki-de
    backend: wiktextract-jsonl
    url: https://example.org/de.jsonl
    language: de
    system: wiktionary_de

query_expansion:
  backend: foo
  url: https://example.org
  max_candidates: 10
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml_content.as_bytes()).unwrap();
        file.flush().unwrap();

        let result = load_sources(file.path());
        assert!(result.is_err());
        // The error should be about unknown backend value
        // Either from serde (during parse) or from our validation
    }
}
