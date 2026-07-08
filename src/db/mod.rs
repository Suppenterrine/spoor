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
pub struct ImportReport {
    pub imported: usize,
    pub unknown_class: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WordRecord {
    pub id: String,
    pub word: String,
    pub word_class: Option<String>,
    pub language: Option<String>,
    pub system: Option<String>,
    pub tags: Option<String>,
    pub seed_weight: f64,
    pub source: Option<String>,
    pub etymology: Option<String>,
    pub origin_lang: Option<String>,
}

impl WordRecord {
    /// Convert non-empty strings to Some, empty or whitespace-only to None
    fn non_empty(s: Option<&str>) -> Option<String> {
        s.and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }

    pub fn parse_csv_record(r: &csv::StringRecord) -> anyhow::Result<Self> {
        let word = r.get(0).map(|s| s.trim().to_string()).unwrap_or_default();
        let language = Self::non_empty(r.get(1));
        let word_class = Self::non_empty(r.get(2));
        let system = Self::non_empty(r.get(3));
        let tags = Self::non_empty(r.get(4));
        let seed_weight = r
            .get(5)
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(1.0);
        let source = Self::non_empty(r.get(6));
        let etymology = Self::non_empty(r.get(7));
        let origin_lang = Self::non_empty(r.get(8));

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
            etymology,
            origin_lang,
        })
    }
}

const INSERT_WORD_SQL: &str = "INSERT OR REPLACE INTO words (id, word, word_class, language, system, tags, seed_weight, source, etymology, origin_lang)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";

/// Execute the shared word-insert statement for one record.
fn execute_insert(stmt: &mut rusqlite::Statement, rec: &WordRecord) -> anyhow::Result<()> {
    stmt.execute(params![
        rec.id,
        rec.word,
        rec.word_class,
        rec.language,
        rec.system,
        rec.tags,
        rec.seed_weight,
        rec.source,
        rec.etymology,
        rec.origin_lang
    ])
    .with_context(|| format!("failed to insert word: {}", rec.word))?;
    Ok(())
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

    fn ensure_column(&mut self, table: &str, column: &str, decl: &str) -> anyhow::Result<()> {
        // Check if column exists by querying PRAGMA table_info
        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA table_info({})", table))
            .context("failed to prepare PRAGMA table_info")?;

        let mut exists = false;
        let mut rows = stmt
            .query([])
            .context("failed to query table_info")?;

        while let Some(row) = rows.next().context("failed to read table_info row")? {
            let col_name: String = row.get(1).context("failed to get column name")?;
            if col_name == column {
                exists = true;
                break;
            }
        }

        if !exists {
            self.conn
                .execute(
                    &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, decl),
                    [],
                )
                .with_context(|| format!("failed to add column {} to {}", column, table))?;
        }
        Ok(())
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

        // Migrate: add etymology and origin_lang columns if they don't exist
        self.ensure_column("words", "etymology", "TEXT")?;
        self.ensure_column("words", "origin_lang", "TEXT")?;

        Ok(())
    }

    pub fn insert_words(&mut self, records: &[WordRecord]) -> anyhow::Result<()> {
        let tx = self.conn.transaction().context("failed to start insert transaction")?;
        let mut stmt = tx
            .prepare(INSERT_WORD_SQL)
            .context("failed to prepare insert statement")?;
        for rec in records {
            execute_insert(&mut stmt, rec)?;
        }
        drop(stmt);
        tx.commit().context("failed to commit insert transaction")?;
        Ok(())
    }

    /// Stream-import CSV file into database. Counts unrecognized word_class values.
    /// Valid word classes: prefix, noun, proper, adj, suffix, suffix_noun
    pub fn import_csv(&mut self, path: impl AsRef<Path>) -> anyhow::Result<ImportReport> {
        let reader = csv::Reader::from_path(path).context("failed to open CSV file")?;
        let tx = self.conn.transaction().context("failed to start import transaction")?;
        let mut stmt = tx
            .prepare(INSERT_WORD_SQL)
            .context("failed to prepare import statement")?;

        let mut imported = 0;
        let mut unknown_class = 0;

        for result in reader.into_records() {
            let record = result.context("failed to read CSV record")?;
            let rec = WordRecord::parse_csv_record(&record)?;

            // Check if word_class is recognized
            if let Some(ref wc) = rec.word_class {
                if !matches!(
                    wc.as_str(),
                    "prefix" | "noun" | "proper" | "adj" | "suffix" | "suffix_noun"
                ) {
                    unknown_class += 1;
                }
            }

            // Always insert the record
            execute_insert(&mut stmt, &rec)?;
            imported += 1;
        }

        drop(stmt);
        tx.commit().context("failed to commit import transaction")?;
        Ok(ImportReport {
            imported,
            unknown_class,
        })
    }

    /// Map a database row to a WordRecord
    fn map_word_row(row: &rusqlite::Row) -> rusqlite::Result<WordRecord> {
        Ok(WordRecord {
            id: row.get(0)?,
            word: row.get(1)?,
            word_class: row.get(2)?,
            language: row.get(3)?,
            system: row.get(4)?,
            tags: row.get(5)?,
            seed_weight: row.get(6)?,
            source: row.get(7)?,
            etymology: row.get(8)?,
            origin_lang: row.get(9)?,
        })
    }

    /// Helper: build WHERE system IN (...) placeholder string for n systems
    fn in_clause(n: usize) -> String {
        (1..=n)
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Retrieve all word records from the database, optionally filtered by systems, ordered by id
    pub fn all_records(&self, systems: Option<&[String]>) -> anyhow::Result<Vec<WordRecord>> {
        let systems = systems.unwrap_or(&[]);
        let query = if systems.is_empty() {
            "SELECT id, word, word_class, language, system, tags, seed_weight, source, etymology, origin_lang FROM words ORDER BY id".to_string()
        } else {
            format!(
                "SELECT id, word, word_class, language, system, tags, seed_weight, source, etymology, origin_lang FROM words WHERE system IN ({}) ORDER BY id",
                Self::in_clause(systems.len())
            )
        };

        let mut stmt = self
            .conn
            .prepare(&query)
            .context("failed to prepare all_records query")?;

        let rows = stmt
            .query_map(rusqlite::params_from_iter(systems.iter().map(|s| s.as_str())), Self::map_word_row)
            .context("failed to query all records")?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row.context("failed to read word row")?);
        }
        Ok(out)
    }

    pub fn words_by_class(&self, systems: Option<&[String]>) -> anyhow::Result<Vec<(String, String)>> {
        let systems = systems.unwrap_or(&[]);
        let query = if systems.is_empty() {
            "SELECT word, word_class FROM words ORDER BY id".to_string()
        } else {
            format!(
                "SELECT word, word_class FROM words WHERE system IN ({}) ORDER BY id",
                Self::in_clause(systems.len())
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

        // Collect parameters in order, using params_from_iter with Option values
        let params_iter = system.into_iter().chain(language.into_iter());

        let rows = stmt
            .query_map(rusqlite::params_from_iter(params_iter), |row| {
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
