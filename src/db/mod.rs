use std::{collections::HashMap, path::Path};

use anyhow::Context;
use rusqlite::{self, Connection, params};

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

    pub fn get_random_by_system(
        &self,
        system: &str,
        _rng: &mut crate::generator::SeededRng,
        limit: usize,
    ) -> anyhow::Result<Vec<WordRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, word, word_class, language, system, tags, seed_weight, source FROM words WHERE system = ?1 ORDER BY RANDOM() LIMIT ?2")
            .context("failed to prepare random-by-system query")?;

        let rows = stmt
            .query_map(params![system, limit as i64], |row| {
                Ok(WordRecord {
                    id: row.get(0)?,
                    word: row.get(1)?,
                    word_class: row.get(2)?,
                    language: row.get(3)?,
                    system: row.get(4)?,
                    tags: row.get(5)?,
                    seed_weight: row.get(6)?,
                    source: row.get(7)?,
                })
            })
            .context("failed to query random words by system")?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn search_by_tag(&self, tag: &str) -> anyhow::Result<Vec<WordRecord>> {
        let pattern = format!("%{}%", tag);
        let mut stmt = self
            .conn
            .prepare("SELECT id, word, word_class, language, system, tags, seed_weight, source FROM words WHERE tags LIKE ?1")
            .context("failed to prepare tag search query")?;
        let rows = stmt
            .query_map(params![pattern], |row| {
                Ok(WordRecord {
                    id: row.get(0)?,
                    word: row.get(1)?,
                    word_class: row.get(2)?,
                    language: row.get(3)?,
                    system: row.get(4)?,
                    tags: row.get(5)?,
                    seed_weight: row.get(6)?,
                    source: row.get(7)?,
                })
            })
            .context("failed to search words by tag")?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn stats(&self) -> anyhow::Result<HashMap<String, usize>> {
        let mut stmt = self
            .conn
            .prepare("SELECT language, COUNT(*) FROM words GROUP BY language")
            .context("failed to prepare stats query")?;
        let mut out = HashMap::new();
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
            })
            .context("failed to collect stats rows")?;
        for row in rows {
            let (lang, count) = row?;
            out.insert(lang, count);
        }
        let total = out.values().sum();
        out.insert("total".into(), total);
        Ok(out)
    }

    pub fn total(&self) -> anyhow::Result<usize> {
        let count: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM words", [], |row| row.get(0))
            .context("failed to count words")?;
        Ok(count)
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}
