use crate::db::WordRecord;
use std::collections::HashSet;

pub struct Match {
    pub record: WordRecord,
    pub score: f64,
    pub matched: Vec<String>, // which query tokens hit, with the field, e.g. "sky (tag)"
}

const STOPWORDS: &[&str] = &[
    // German
    "der", "die", "das", "ein", "eine", "und", "oder", "für", "mit", "von", "zu", "im", "am",
    "auf",
    // English
    "the", "a", "an", "of", "for", "and", "or", "to", "in", "on", "with", "that", "is",
    // Specific to domain
    "app", "tool",
];

/// Lowercase, split on non-alphanumeric (keep umlauts/unicode letters), drop stopwords, dedup preserving order
pub fn tokenize(query: &str) -> Vec<String> {
    let lower = query.to_lowercase();
    let mut tokens = Vec::new();
    let mut current_token = String::new();

    for ch in lower.chars() {
        if ch.is_alphanumeric() || ch == 'ü' || ch == 'ö' || ch == 'ä' || ch == 'ß' {
            current_token.push(ch);
        } else {
            if !current_token.is_empty() {
                if !STOPWORDS.contains(&current_token.as_str()) {
                    tokens.push(current_token.clone());
                }
                current_token.clear();
            }
        }
    }

    if !current_token.is_empty() {
        if !STOPWORDS.contains(&current_token.as_str()) {
            tokens.push(current_token);
        }
    }

    // Dedup while preserving order
    let mut seen = HashSet::new();
    tokens.into_iter().filter(|t| seen.insert(t.clone())).collect()
}

/// Exact or bidirectional-PREFIX match of a token against one text.
/// Prefix instead of substring keeps mid-word noise out ("cli" must not hit
/// "acclimatation"); both sides need at least 3 chars for a partial hit.
fn score_text(text: &str, token: &str, exact: f64, partial: f64) -> f64 {
    if text == token {
        exact
    } else if token.len() >= 3 && text.len() >= 3 && (text.starts_with(token) || token.starts_with(text)) {
        partial
    } else {
        0.0
    }
}

/// Minimum token length for free substring matching in prose fields
/// (tags/glosses, etymology) — short tokens produce too many phantom hits.
const MIN_SUBSTRING_TOKEN: usize = 4;

/// Best score of a token against a list of texts (e.g. tags/glosses).
/// Exact and prefix hits via score_text; additionally a substring hit inside
/// prose items (glosses are whole phrases) for tokens of useful length.
fn score_list(items: &[String], token: &str, exact: f64, partial: f64) -> f64 {
    items
        .iter()
        .map(|item| {
            let s = score_text(item, token, exact, partial);
            if s > 0.0 {
                s
            } else if token.len() >= MIN_SUBSTRING_TOKEN && item.contains(token) {
                partial
            } else {
                0.0
            }
        })
        .fold(0.0, f64::max)
}

/// Pure scoring of one record against tokens; returns (score, matched).
/// Each token scores each field category at most once.
fn score_record(record: &WordRecord, tokens: &[String]) -> (f64, Vec<String>) {
    let word = record.word.to_lowercase();
    let tags: Vec<String> = record
        .tags
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    let system = record.system.as_deref().map(str::to_lowercase);
    let etymology = record.etymology.as_deref().map(str::to_lowercase);

    let mut total_score = 0.0;
    let mut matched = Vec::new();

    for token in tokens {
        let field_scores = [
            ("word", score_text(&word, token, 5.0, 2.0)),
            ("tag", score_list(&tags, token, 3.0, 1.5)),
            (
                "system",
                system.as_deref().map_or(0.0, |s| score_text(s, token, 2.0, 2.0)),
            ),
            (
                "etymology",
                etymology.as_deref().map_or(0.0, |e| {
                    if token.len() >= MIN_SUBSTRING_TOKEN && e.contains(token.as_str()) {
                        1.0
                    } else {
                        0.0
                    }
                }),
            ),
        ];

        for (field, score) in field_scores {
            if score > 0.0 {
                total_score += score;
                matched.push(format!("{} ({})", token, field));
            }
        }
    }

    (total_score * record.seed_weight, matched)
}

/// Tokenize + score all + filter score > 0 + sort by score DESC, then seed_weight DESC, then word ASC (fully deterministic)
pub fn rank(records: &[WordRecord], query: &str) -> Vec<Match> {
    let tokens = tokenize(query);

    let mut matches = Vec::new();
    for record in records {
        let (score, matched) = score_record(record, &tokens);
        if score > 0.0 {
            matches.push(Match {
                record: record.clone(),
                score,
                matched,
            });
        }
    }

    // Sort by score DESC, then seed_weight DESC, then word ASC (fully deterministic)
    matches.sort_by(|a, b| {
        let cmp_score = b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal);
        if cmp_score != std::cmp::Ordering::Equal {
            return cmp_score;
        }

        let cmp_weight = b.record.seed_weight.partial_cmp(&a.record.seed_weight)
            .unwrap_or(std::cmp::Ordering::Equal);
        if cmp_weight != std::cmp::Ordering::Equal {
            return cmp_weight;
        }

        a.record.word.cmp(&b.record.word)
    });

    matches
}

/// German one-liner justification: "<word> — <etymology> (<origin_lang>) · System: <system> · Treffer: <matched joined>"
pub fn explain(m: &Match) -> String {
    let word = &m.record.word;
    let etymology = m.record.etymology.as_deref().unwrap_or("?");
    let origin_lang = m.record.origin_lang.as_deref().unwrap_or("?");
    let system = m.record.system.as_deref().unwrap_or("?");
    let matched = m.matched.join(", ");

    format!(
        "{} — {} ({}) · System: {} · Treffer: {}",
        word, etymology, origin_lang, system, matched
    )
}
