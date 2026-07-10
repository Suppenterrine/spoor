use crate::db::{Edge, WordRecord};
use std::collections::{BTreeSet, HashMap, HashSet};

pub struct Match {
    pub record: WordRecord,
    pub score: f64,
    /// Which query tokens/concepts hit, with the field or bridge path,
    /// e.g. "sky (glosse)" or "baum → tree (bruecke)"
    pub matched: Vec<String>,
}

const STOPWORDS: &[&str] = &[
    // German
    "der", "die", "das", "ein", "eine", "und", "oder", "für", "mit", "von", "zu", "im", "am",
    "auf", "aus", "dem", "den", "des", "einer", "eines", "über", "unter", "bei", "nach",
    "vor", "durch", "als", "wie", "ist", "sind", "wird", "werden",
    // English
    "the", "a", "an", "of", "for", "and", "or", "to", "in", "on", "with", "that", "is",
    "it", "its", "be", "by", "at", "as", "this", "these", "are", "from", "into", "over",
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
/// (etymology) — short tokens produce too many phantom hits.
const MIN_SUBSTRING_TOKEN: usize = 4;

/// Noise words that dominate dictionary glosses; they carry no concept on
/// their own and would bridge everything to everything.
const GLOSS_NOISE: &[&str] = &[
    "one", "who", "which", "used", "being", "having", "act", "state",
    "quality", "person", "something", "someone", "any", "other", "such",
    "made", "given", "form", "type", "kind", "member", "unit", "way",
    "esp", "especially", "usually", "often", "certain", "various",
    "general", "also", "etc", "sense", "senses", "term", "word",
];

fn is_concept_token(t: &str) -> bool {
    t.chars().count() >= 3 && !STOPWORDS.contains(&t) && !GLOSS_NOISE.contains(&t)
}

/// Tokenize the gloss phrases stored in `tags` into a deduplicated,
/// order-stable list of concept tokens.
fn gloss_tokens(record: &WordRecord) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    if let Some(tags) = record.tags.as_deref() {
        for phrase in tags.split(',') {
            for tok in tokenize(phrase) {
                if is_concept_token(&tok) && seen.insert(tok.clone()) {
                    out.push(tok);
                }
            }
        }
    }
    out
}

/// Strip a plural-ish trailing 's' so "logs"/"log" and "trees"/"tree" compare equal.
fn singularish(t: &str) -> &str {
    if t.len() > 3 && t.ends_with('s') {
        &t[..t.len() - 1]
    } else {
        t
    }
}

/// Concept-level term equality: exact after plural-strip, or bidirectional
/// prefix for longer terms (catches simple inflection differences).
fn terms_match(a: &str, b: &str) -> bool {
    let (a, b) = (singularish(a), singularish(b));
    a == b || (a.len() >= 5 && b.len() >= 5 && (a.starts_with(b) || b.starts_with(a)))
}

/// How a concept was derived from the query; carries the path shown in the
/// Spur output.
#[derive(Clone, PartialEq)]
enum ConceptKind {
    /// A query token itself
    Token,
    /// Reached over the gloss bridge: via = the query token
    Bridge { via: String },
    /// Datamuse candidate (--online / auto-online)
    Online,
    /// Reached over a nexus edge: via = the edge source, rel = relation name
    Assoc { via: String, rel: String },
}

/// A weighted concept derived from the query.
struct Concept {
    term: String,
    weight: f64,
    kind: ConceptKind,
}

/// Query tokens count full; concepts reached over the gloss bridge or the
/// online expansion count less, since they are one association step away.
const BRIDGE_CONCEPT_WEIGHT: f64 = 0.7;
const ONLINE_CONCEPT_WEIGHT: f64 = 0.6;
/// Cap gloss-token fan-out per query token so one polysemous bridge word
/// cannot flood the concept list.
const MAX_BRIDGE_CONCEPTS_PER_TOKEN: usize = 8;
/// Caps for nexus-edge expansion: per edge source and in total.
const MAX_ASSOC_PER_SRC: usize = 6;
const MAX_ASSOC_TOTAL: usize = 24;

/// Base score of a concept hitting a record's gloss (multiplied by concept
/// weight and the gloss token's IDF) and of a concept naming the record's
/// word directly (a translation hit).
const CONCEPT_GLOSS_BASE: f64 = 1.2;
const CONCEPT_WORD_HIT: f64 = 4.0;
/// Dampening for words that merely contain a query token (compounds).
const NEAR_ECHO_FACTOR: f64 = 0.3;

/// Stage A of the lookup: turn query tokens into weighted concepts.
///
/// Every query token is a concept itself. Tokens that name a word in
/// `bridge_records` (any language) additionally expand to that word's
/// English gloss tokens — the concept bridge: "baum" → de/Baum → "tree".
/// Nexus edges then add association concepts (1 hop from tokens or bridge
/// concepts, weight decaying with both the base concept and the relation).
/// Datamuse candidates are appended with the lowest weight.
fn build_concepts(
    tokens: &[String],
    bridge_records: &[WordRecord],
    candidates: &[String],
    edges: &[Edge],
) -> Vec<Concept> {
    let mut concepts: Vec<Concept> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for t in tokens {
        if seen.insert(t.clone()) {
            concepts.push(Concept {
                term: t.clone(),
                weight: 1.0,
                kind: ConceptKind::Token,
            });
        }
    }

    for t in tokens {
        let mut added = 0usize;
        for record in bridge_records {
            if record.word.to_lowercase() != *t {
                continue;
            }
            for g in gloss_tokens(record) {
                if added >= MAX_BRIDGE_CONCEPTS_PER_TOKEN {
                    break;
                }
                if seen.insert(g.clone()) {
                    concepts.push(Concept {
                        term: g,
                        weight: BRIDGE_CONCEPT_WEIGHT,
                        kind: ConceptKind::Bridge { via: t.clone() },
                    });
                    added += 1;
                }
            }
            if added >= MAX_BRIDGE_CONCEPTS_PER_TOKEN {
                break;
            }
        }
    }

    // Association hop over nexus edges: an edge counts if its source is an
    // existing token/bridge concept; the new weight decays with both.
    // Targets that CONTAIN a query token (compounds like "apfelbaum" for
    // "baum") are skipped — they are near-echoes, not associations.
    let base_weights: HashMap<String, f64> = concepts
        .iter()
        .map(|c| (c.term.clone(), c.weight))
        .collect();
    let mut assoc_total = 0usize;
    let mut assoc_per_src: HashMap<&str, usize> = HashMap::new();
    for edge in edges {
        if assoc_total >= MAX_ASSOC_TOTAL {
            break;
        }
        let Some(base) = base_weights.get(edge.src.as_str()) else { continue };
        let per_src = assoc_per_src.entry(edge.src.as_str()).or_insert(0);
        if *per_src >= MAX_ASSOC_PER_SRC {
            continue;
        }
        if !is_concept_token(&edge.dst) {
            continue;
        }
        if tokens.iter().any(|t| t.len() >= 4 && edge.dst.contains(t.as_str())) {
            continue;
        }
        if seen.insert(edge.dst.clone()) {
            concepts.push(Concept {
                term: edge.dst.clone(),
                weight: base * edge.weight,
                kind: ConceptKind::Assoc {
                    via: edge.src.clone(),
                    rel: edge.rel.clone(),
                },
            });
            *per_src += 1;
            assoc_total += 1;
        }
    }

    for c in candidates {
        let lc = c.to_lowercase();
        if is_concept_token(&lc) && seen.insert(lc.clone()) {
            concepts.push(Concept {
                term: lc,
                weight: ONLINE_CONCEPT_WEIGHT,
                kind: ConceptKind::Online,
            });
        }
    }

    concepts
}

/// The terms whose outgoing nexus edges matter for a query: the tokens plus
/// their bridge concepts. The CLI uses this to load only the relevant edge
/// rows from the database before ranking.
pub fn edge_source_terms(query: &str, bridge_records: &[WordRecord]) -> Vec<String> {
    build_concepts(&tokenize(query), bridge_records, &[], &[])
        .into_iter()
        .map(|c| c.term)
        .collect()
}

/// Smoothed inverse document frequency of a gloss token: common gloss words
/// ("water", "plant") score near 1, rare ones score higher. Never negative,
/// and 1.0 in degenerate corpora (n == df), so tiny test fixtures still
/// match. Capped so one-off tokens cannot dominate the whole ranking.
fn idf(df: &HashMap<&str, usize>, n: f64, term: &str) -> f64 {
    let d = df.get(term).copied().unwrap_or(1).max(1) as f64;
    1.0 + (n / d).ln().clamp(0.0, 5.0)
}

/// Stage B: score one record against direct tokens (word prefix, system,
/// etymology) and concepts (gloss tokens with IDF, word == concept).
/// Pure; echo handling and weight/origin factors live in `rank_semantic`.
fn score_record_semantic(
    record: &WordRecord,
    gloss_toks: &[String],
    tokens: &[String],
    concepts: &[Concept],
    df: &HashMap<&str, usize>,
    n: f64,
) -> (f64, Vec<String>) {
    let word = record.word.to_lowercase();
    // Folded Latin form so "logos" (query/concept) hits grc λόγος (lógos)
    let word_latin = record
        .translit
        .as_deref()
        .map(|t| crate::translit::fold_diacritics(&t.to_lowercase()));
    let system = record.system.as_deref().map(str::to_lowercase);
    let etymology = record.etymology.as_deref().map(str::to_lowercase);

    let names_record = |term: &str| word == term || word_latin.as_deref() == Some(term);

    let mut score = 0.0;
    let mut matched = Vec::new();

    for token in tokens {
        if word != *token {
            let s = score_text(&word, token, 0.0, 1.5);
            if s > 0.0 {
                score += s;
                matched.push(format!("{} (wort~)", token));
            }
        }
        if let Some(sys) = system.as_deref() {
            let s = score_text(sys, token, 2.0, 2.0);
            if s > 0.0 {
                score += s;
                matched.push(format!("{} (system)", token));
            }
        }
        if let Some(e) = etymology.as_deref() {
            if token.len() >= MIN_SUBSTRING_TOKEN && e.contains(token.as_str()) {
                score += 1.0;
                matched.push(format!("{} (etymologie)", token));
            }
        }
    }

    // One concept is one piece of evidence — and one gloss token yields
    // evidence only once per record: otherwise several concepts sharing a
    // prefix (e.g. five compounds derived from the same root) all hit the
    // same gloss token and stack up phantom score.
    let mut consumed_gloss: HashSet<usize> = HashSet::new();
    for c in concepts {
        let gloss_hit = gloss_toks
            .iter()
            .enumerate()
            .find(|(i, g)| !consumed_gloss.contains(i) && terms_match(&c.term, g))
            .map(|(i, g)| (i, CONCEPT_GLOSS_BASE * c.weight * idf(df, n, g)));

        // Concept names the record's word: a translation hit ("tree" → en/tree).
        // Direct query tokens are excluded here — that is the echo case,
        // handled in rank_semantic.
        let word_hit = if names_record(&c.term) && c.kind != ConceptKind::Token {
            Some(CONCEPT_WORD_HIT * c.weight)
        } else {
            None
        };

        let gloss_score = gloss_hit.map(|(_, s)| s).unwrap_or(0.0);
        let best = gloss_score.max(word_hit.unwrap_or(0.0));
        if best > 0.0 {
            score += best;
            let is_word_hit = word_hit.unwrap_or(0.0) >= gloss_score;
            if !is_word_hit {
                if let Some((i, _)) = gloss_hit {
                    consumed_gloss.insert(i);
                }
            }
            matched.push(match &c.kind {
                ConceptKind::Token => format!("{} (glosse)", c.term),
                ConceptKind::Bridge { via } if is_word_hit => {
                    format!("{} → {} (wort)", via, c.term)
                }
                ConceptKind::Bridge { via } => format!("{} → {} (bruecke)", via, c.term),
                ConceptKind::Online => format!("{} (semantisch)", c.term),
                ConceptKind::Assoc { via, rel } => format!("{} → {} ({})", via, c.term, rel),
            });
        }
    }

    (score, matched)
}

/// Languages whose words read as origin material: the North Star prefers
/// roots from old/origin languages over echoing the query language back.
fn origin_factor(record: &WordRecord) -> f64 {
    match record.language.as_deref() {
        Some(
            "la" | "grc" | "he" | "non" | "ang" | "goh" // klassisch + altgermanisch
            | "sa" | "sux" | "akk" | "egy" | "got" // Sanskrit, Sumerisch, Akkadisch, Ägyptisch, Gotisch
            | "nci" | "yua" | "qu", // Klassisches Nahuatl, Yukatekisches Maya, Quechua
        ) => 1.3,
        Some("el") => 1.15,
        _ => 1.0,
    }
}

/// Poetic, literary and figurative words make better names — small boost.
fn register_factor(record: &WordRecord) -> f64 {
    match record.registers.as_deref() {
        Some(r) if r.contains("poetic") || r.contains("literary") || r.contains("figurative") => {
            1.15
        }
        _ => 1.0,
    }
}

/// Full semantic ranking: concept bridge + IDF gloss scoring + anti-echo +
/// origin bonus. `bridge_records` is the vocabulary used for concept
/// extraction (usually the whole database, even when `records` is filtered
/// by --systems). Deterministic: score DESC, seed_weight DESC, word ASC.
///
/// Anti-echo: a record whose word equals a query token is excluded unless
/// `allow_echo` — the North Star wants association, not the query back.
pub fn rank_semantic(
    records: &[WordRecord],
    bridge_records: &[WordRecord],
    query: &str,
    candidates: &[String],
    edges: &[Edge],
    allow_echo: bool,
) -> Vec<Match> {
    let tokens = tokenize(query);
    let concepts = build_concepts(&tokens, bridge_records, candidates, edges);

    let record_gloss: Vec<Vec<String>> = records.iter().map(gloss_tokens).collect();
    let n = records.len().max(1) as f64;
    let mut df: HashMap<&str, usize> = HashMap::new();
    for toks in &record_gloss {
        for t in toks {
            *df.entry(t.as_str()).or_insert(0) += 1;
        }
    }

    let mut matches = Vec::new();
    for (record, gloss_toks) in records.iter().zip(&record_gloss) {
        let word_lower = record.word.to_lowercase();
        let word_latin = record
            .translit
            .as_deref()
            .map(|t| crate::translit::fold_diacritics(&t.to_lowercase()));
        let is_echo = tokens
            .iter()
            .any(|t| *t == word_lower || word_latin.as_deref() == Some(t.as_str()));
        if is_echo && !allow_echo {
            continue;
        }
        // Near-echo (word CONTAINS a query token, e.g. "Apfelbaum" for
        // "baum"): not excluded, but strongly dampened — the North Star
        // wants associations, not compounds of the query word.
        let is_near_echo = !allow_echo
            && tokens
                .iter()
                .any(|t| t.len() >= 4 && word_lower.contains(t.as_str()));

        let (mut score, mut matched) =
            score_record_semantic(record, gloss_toks, &tokens, &concepts, &df, n);

        if is_echo {
            // allow_echo: restore the old exact-word behavior.
            score += 5.0;
            matched.push(format!("{} (wort)", word_lower));
        }

        score *= record.seed_weight * origin_factor(record) * register_factor(record);
        if is_near_echo {
            score *= NEAR_ECHO_FACTOR;
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

/// Ranking with Datamuse candidates; the records themselves double as the
/// bridge vocabulary. Kept for callers without a separate bridge set.
pub fn rank_with_candidates(records: &[WordRecord], query: &str, candidates: &[String]) -> Vec<Match> {
    rank_semantic(records, records, query, candidates, &[], false)
}

/// Plain ranking without candidates or edges. Wrapper around rank_semantic.
pub fn rank(records: &[WordRecord], query: &str) -> Vec<Match> {
    rank_semantic(records, records, query, &[], &[], false)
}

/// German one-liner justification with a caller-chosen display form of the
/// word (e.g. the Latin transliteration):
/// "<display> — <etymology> (<origin_lang>) · System: <system> · Spur: <matched joined>"
/// Missing fields are omitted instead of printed as "?".
pub fn explain_display(m: &Match, display_word: &str) -> String {
    let mut parts = Vec::new();

    match (m.record.etymology.as_deref(), m.record.origin_lang.as_deref()) {
        (Some(e), Some(o)) => parts.push(format!("{} ({})", e, o)),
        (Some(e), None) => parts.push(e.to_string()),
        (None, Some(o)) => parts.push(format!("({})", o)),
        (None, None) => {}
    }
    if let Some(system) = m.record.system.as_deref() {
        parts.push(format!("System: {}", system));
    }
    parts.push(format!("Spur: {}", m.matched.join(", ")));

    format!("{} — {}", display_word, parts.join(" · "))
}

/// German one-liner justification using the record's native word form.
pub fn explain(m: &Match) -> String {
    explain_display(m, &m.record.word)
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
                translit: None,
                registers: None,
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
                translit: None,
                registers: None,
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
                translit: None,
                registers: None,
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
                translit: None,
                registers: None,
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
                translit: None,
                registers: None,
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
                translit: None,
                registers: None,
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
                translit: None,
                registers: None,
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
                translit: None,
                registers: None,
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
                translit: None,
                registers: None,
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
                translit: None,
                registers: None,
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
                translit: None,
                registers: None,
            },
        ];

        let candidates = vec!["forest".to_string()];
        let result = rank_with_candidates(&records, "trees", &candidates);

        assert_eq!(result.len(), 1);
        assert!(result[0].matched.iter().any(|m| m.contains("semantisch")));
    }
}
