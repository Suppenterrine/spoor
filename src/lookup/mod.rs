use crate::db::WordRecord;
use std::collections::{HashSet, BTreeSet};

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

/// Generalized ranking with candidate matching support.
/// Takes semantic candidates (from --online expansion) and adds bonus scoring when
/// they exactly match a record word (lowercased). Each candidate counts once per record.
pub fn rank_with_candidates(records: &[WordRecord], query: &str, candidates: &[String]) -> Vec<Match> {
    let tokens = tokenize(query);

    let mut matches = Vec::new();
    for record in records {
        let (mut score, mut matched) = score_record(record, &tokens);

        // Check if any candidate matches this record's word (case-insensitive exact match)
        let word_lower = record.word.to_lowercase();
        for candidate in candidates {
            let candidate_lower = candidate.to_lowercase();
            if word_lower == candidate_lower {
                score += 4.0;
                matched.push(format!("{} (semantisch)", candidate));
                break; // Each candidate counts once per record
            }
        }

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

/// Tokenize + score all + filter score > 0 + sort by score DESC, then seed_weight DESC, then word ASC (fully deterministic)
/// Wrapper around rank_with_candidates with empty candidates for backwards compatibility.
pub fn rank(records: &[WordRecord], query: &str) -> Vec<Match> {
    rank_with_candidates(records, query, &[])
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

/// Calculate the Levenshtein distance between two strings using dynamic programming.
/// Returns the minimum number of single-character edits required to transform one string into another.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

    // Initialize first row and column
    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    // Fill the matrix
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            matrix[i][j] = std::cmp::min(
                std::cmp::min(
                    matrix[i - 1][j] + 1,     // deletion
                    matrix[i][j - 1] + 1,     // insertion
                ),
                matrix[i - 1][j - 1] + cost,  // substitution
            );
        }
    }

    matrix[a_len][b_len]
}

/// Find similar words from the record list for a query.
/// For each token in the query, collects words that either:
/// - start with the first 3 characters of the token (prefix match), OR
/// - have a Levenshtein distance <= 2 from the token
/// Returns up to `max` deduplicated suggestions in alphabetical order.
pub fn suggest<'a>(records: &'a [WordRecord], query: &str, max: usize) -> Vec<&'a str> {
    let tokens = tokenize(query);

    let mut suggestions = BTreeSet::new();

    for token in tokens {
        if token.is_empty() {
            continue;
        }

        let prefix = if token.len() >= 3 {
            &token[..3]
        } else {
            &token
        };

        for record in records {
            let word_lower = record.word.to_lowercase();

            // Prefix match: first 3 chars (or less if token is short)
            if word_lower.starts_with(prefix) {
                suggestions.insert(record.word.as_str());
                if suggestions.len() >= max {
                    break;
                }
            }
            // Levenshtein distance match
            else if levenshtein_distance(&word_lower, &token) <= 2 {
                suggestions.insert(record.word.as_str());
                if suggestions.len() >= max {
                    break;
                }
            }
        }

        if suggestions.len() >= max {
            break;
        }
    }

    suggestions.into_iter().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance_identical() {
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
    }

    #[test]
    fn test_levenshtein_distance_empty() {
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("", ""), 0);
    }

    #[test]
    fn test_levenshtein_distance_one_substitution() {
        assert_eq!(levenshtein_distance("abc", "adc"), 1);
    }

    #[test]
    fn test_levenshtein_distance_one_deletion() {
        assert_eq!(levenshtein_distance("abcd", "acd"), 1);
    }

    #[test]
    fn test_levenshtein_distance_one_insertion() {
        assert_eq!(levenshtein_distance("abd", "abcd"), 1);
    }

    #[test]
    fn test_levenshtein_distance_multiple_edits() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn test_suggest_prefix_match() {
        let records = vec![
            WordRecord {
                id: "test1".to_string(),
                word: "zeus".to_string(),
                word_class: Some("noun".to_string()),
                language: Some("grc".to_string()),
                system: Some("test".to_string()),
                tags: None,
                seed_weight: 1.0,
                source: None,
                etymology: None,
                origin_lang: None,
            },
            WordRecord {
                id: "test2".to_string(),
                word: "hera".to_string(),
                word_class: Some("noun".to_string()),
                language: Some("grc".to_string()),
                system: Some("test".to_string()),
                tags: None,
                seed_weight: 1.0,
                source: None,
                etymology: None,
                origin_lang: None,
            },
        ];

        let result = suggest(&records, "zeu", 10);
        assert!(result.contains(&"zeus"), "should suggest zeus for prefix 'zeu'");
    }

    #[test]
    fn test_suggest_levenshtein_distance() {
        let records = vec![
            WordRecord {
                id: "test1".to_string(),
                word: "zeus".to_string(),
                word_class: Some("noun".to_string()),
                language: Some("grc".to_string()),
                system: Some("test".to_string()),
                tags: None,
                seed_weight: 1.0,
                source: None,
                etymology: None,
                origin_lang: None,
            },
        ];

        let result = suggest(&records, "zeuss", 10); // 1 extra char
        assert!(result.contains(&"zeus"), "should suggest zeus for 'zeuss' within distance 2");
    }

    #[test]
    fn test_suggest_deduplication() {
        let records = vec![
            WordRecord {
                id: "test1".to_string(),
                word: "zeus".to_string(),
                word_class: Some("noun".to_string()),
                language: Some("grc".to_string()),
                system: Some("test".to_string()),
                tags: None,
                seed_weight: 1.0,
                source: None,
                etymology: None,
                origin_lang: None,
            },
            WordRecord {
                id: "test2".to_string(),
                word: "apollo".to_string(),
                word_class: Some("noun".to_string()),
                language: Some("grc".to_string()),
                system: Some("test".to_string()),
                tags: None,
                seed_weight: 1.0,
                source: None,
                etymology: None,
                origin_lang: None,
            },
        ];

        let result = suggest(&records, "zeu apo", 10);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"zeus"));
        assert!(result.contains(&"apollo"));
    }

    #[test]
    fn test_suggest_alphabetical_order() {
        let records = vec![
            WordRecord {
                id: "test1".to_string(),
                word: "beta".to_string(),
                word_class: Some("noun".to_string()),
                language: Some("grc".to_string()),
                system: Some("test".to_string()),
                tags: None,
                seed_weight: 1.0,
                source: None,
                etymology: None,
                origin_lang: None,
            },
            WordRecord {
                id: "test2".to_string(),
                word: "alpha".to_string(),
                word_class: Some("noun".to_string()),
                language: Some("grc".to_string()),
                system: Some("test".to_string()),
                tags: None,
                seed_weight: 1.0,
                source: None,
                etymology: None,
                origin_lang: None,
            },
        ];

        let result = suggest(&records, "bet alp", 10);
        // Should be in alphabetical order
        assert_eq!(result, vec!["alpha", "beta"]);
    }

    #[test]
    fn test_rank_with_candidates_empty_candidates_equals_rank() {
        let records = vec![
            WordRecord {
                id: "test1".to_string(),
                word: "zeus".to_string(),
                word_class: Some("noun".to_string()),
                language: Some("grc".to_string()),
                system: Some("test".to_string()),
                tags: Some("king,sky".to_string()),
                seed_weight: 1.0,
                source: None,
                etymology: None,
                origin_lang: None,
            },
        ];

        let rank_result = rank(&records, "sky");
        let rank_with_candidates_result = rank_with_candidates(&records, "sky", &[]);

        assert_eq!(rank_result.len(), rank_with_candidates_result.len());
        assert_eq!(rank_result[0].score, rank_with_candidates_result[0].score);
        assert_eq!(rank_result[0].record.word, rank_with_candidates_result[0].record.word);
    }

    #[test]
    fn test_rank_with_candidates_exact_match() {
        let records = vec![
            WordRecord {
                id: "test1".to_string(),
                word: "forest".to_string(),
                word_class: Some("noun".to_string()),
                language: Some("en".to_string()),
                system: Some("test".to_string()),
                tags: Some("woods".to_string()),
                seed_weight: 1.0,
                source: None,
                etymology: None,
                origin_lang: None,
            },
            WordRecord {
                id: "test2".to_string(),
                word: "zeus".to_string(),
                word_class: Some("noun".to_string()),
                language: Some("grc".to_string()),
                system: Some("test".to_string()),
                tags: Some("king,sky".to_string()),
                seed_weight: 1.0,
                source: None,
                etymology: None,
                origin_lang: None,
            },
        ];

        let candidates = vec!["forest".to_string(), "woodland".to_string()];
        let result = rank_with_candidates(&records, "trees", &candidates);

        // forest should be in results and have higher score due to semantic match
        assert!(result.iter().any(|m| m.record.word == "forest"));
        let forest_match = result.iter().find(|m| m.record.word == "forest").unwrap();
        assert!(forest_match.matched.iter().any(|m| m.contains("semantisch")));
    }

    #[test]
    fn test_rank_with_candidates_case_insensitive() {
        let records = vec![
            WordRecord {
                id: "test1".to_string(),
                word: "Forest".to_string(),
                word_class: Some("noun".to_string()),
                language: Some("en".to_string()),
                system: Some("test".to_string()),
                tags: None,
                seed_weight: 1.0,
                source: None,
                etymology: None,
                origin_lang: None,
            },
        ];

        let candidates = vec!["forest".to_string()];
        let result = rank_with_candidates(&records, "trees", &candidates);

        assert_eq!(result.len(), 1);
        assert!(result[0].matched.iter().any(|m| m.contains("semantisch")));
    }
}
