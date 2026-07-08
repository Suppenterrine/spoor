use std::path::Path;

use anyhow::Context;
use rusqlite::{self, Connection, params};

#[derive(Debug, Clone)]
pub struct DbStats {
    pub total: usize,
    pub by_language: Vec<(String, usize)>,
    pub by_system: Vec<(String, usize)>,
}

#[derive(Debug, Clone)]
pub struct WordRecord {
    pub id: String,
    pub word: String,
    pub word_class: Option<String>,
    pub language: Option<String>,
    pub system: Option<String>,
    pub tags: Option<String>,
    pub seed_weight: f64,
    pub source: Option<String>,
}

impl WordRecord {
    pub fn parse_csv_record(r: &csv::StringRecord) -> anyhow::Result<Self> {
        let word = r.get(0).map(|s| s.trim().to_string()).unwrap_or_default();
        let language = r.get(1).map(|s| s.trim().to_string());
        let word_class = r.get(2).map(|s| s.trim().to_string());
        let system = r.get(3).map(|s| s.trim().to_string());
        let tags = r.get(4).map(|s| s.trim().to_string());
        let seed_weight = r
            .get(5)
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(1.0);
        let source = r.get(6).map(|s| s.trim().to_string());

        let id = if let Some(ref lang) = language {
            format!("{}_{}", lang, word)
        } else {
            word.clone()
        };

        Ok(Self {
            id,
            word,
            word_class,
            language,
            system,
            tags,
            seed_weight,
            source,
        })
    }
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let conn = Connection::open(path).context("failed to open database")?;
        let mut db = Self { conn };
        db.ensure_schema()?;
        Ok(db)
    }

    fn ensure_schema(&mut self) -> anyhow::Result<()> {
        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS words (
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
            .context("failed to create words table")?;
        Ok(())
    }

    pub fn insert_words(&mut self, records: &[WordRecord]) -> anyhow::Result<()> {
        let tx = self.conn.transaction().context("failed to start insert transaction")?;
        for rec in records {
            tx.execute(
                "INSERT OR REPLACE INTO words (id, word, word_class, language, system, tags, seed_weight, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![rec.id, rec.word, rec.word_class, rec.language, rec.system, rec.tags, rec.seed_weight, rec.source],
            )
            .with_context(|| format!("failed to insert word: {}", rec.word))?;
        }
        tx.commit().context("failed to commit insert transaction")?;
        Ok(())
    }

    pub fn words_by_class(&self, systems: Option<&[String]>) -> anyhow::Result<Vec<(String, String)>> {
        let systems = systems.unwrap_or(&[]);
        let query = if systems.is_empty() {
            "SELECT word, word_class FROM words ORDER BY id".to_string()
        } else {
            format!(
                "SELECT word, word_class FROM words WHERE system IN ({}) ORDER BY id",
                (1..=systems.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",")
            )
        };

        let mut stmt = self
            .conn
            .prepare(&query)
            .context("failed to prepare words-by-class query")?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(systems.iter().map(|s| s.as_str())), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .context("failed to query words by class")?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn query_group_by(&self, query: &str) -> anyhow::Result<Vec<(String, usize)>> {
        let mut stmt = self
            .conn
            .prepare(query)
            .context("failed to prepare group-by query")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
            })
            .context("failed to collect group-by rows")?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn stats(&self) -> anyhow::Result<DbStats> {
        let total: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM words", [], |row| row.get(0))
            .context("failed to count words")?;

        let by_language = self.query_group_by(
            "SELECT language, COUNT(*) as cnt FROM words GROUP BY language ORDER BY cnt DESC, language ASC"
        )?;

        let by_system = self.query_group_by(
            "SELECT system, COUNT(*) as cnt FROM words GROUP BY system ORDER BY cnt DESC, system ASC"
        )?;

        Ok(DbStats {
            total,
            by_language,
            by_system,
        })
    }

    /// List all systems with word counts, ordered by count DESC then name ASC.
    pub fn list_systems(&self) -> anyhow::Result<Vec<(String, usize)>> {
        self.query_group_by(
            "SELECT system, COUNT(*) as cnt FROM words GROUP BY system ORDER BY cnt DESC, system ASC"
        )
    }

    /// List all languages with word counts, ordered by count DESC then name ASC.
    pub fn list_languages(&self) -> anyhow::Result<Vec<(String, usize)>> {
        self.query_group_by(
            "SELECT language, COUNT(*) as cnt FROM words GROUP BY language ORDER BY cnt DESC, language ASC"
        )
    }

    /// List all word classes with word counts, ordered by count DESC then name ASC.
    pub fn list_classes(&self) -> anyhow::Result<Vec<(String, usize)>> {
        self.query_group_by(
            "SELECT word_class, COUNT(*) as cnt FROM words GROUP BY word_class ORDER BY cnt DESC, word_class ASC"
        )
    }

    /// List words with optional system and language filters.
    /// Returns (word, language, system, word_class) ordered by word ASC.
    pub fn list_words(&self, system: Option<&str>, language: Option<&str>) -> anyhow::Result<Vec<(String, String, String, String)>> {
        // Build WHERE clause conditions
        let mut conditions = Vec::new();
        if system.is_some() {
            conditions.push("system = ?1");
        }
        if language.is_some() {
            conditions.push(if system.is_some() { "language = ?2" } else { "language = ?1" });
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let query = format!(
            "SELECT word, language, system, word_class FROM words {} ORDER BY word",
            where_clause
        );

        let mut stmt = self
            .conn
            .prepare(&query)
            .context("failed to prepare list-words query")?;

        // Prepare parameters
        let params_vec: Vec<String> = [system.map(|s| s.to_string()), language.map(|l| l.to_string())]
            .into_iter()
            .filter_map(|p| p)
            .collect();

        // Convert to references for query_map
        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

        let rows = stmt
            .query_map(rusqlite::params_from_iter(param_refs), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .context("failed to query words")?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row.context("failed to read word row")?);
        }
        Ok(out)
    }

}
