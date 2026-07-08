use crate::config::Config;
use crate::generator::rng::SeededRng;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct WordLists {
    pub prefixes: Vec<String>,
    pub words: Vec<String>,
    pub suffix_adjs: Vec<String>,
    pub suffix_names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Prefix,
    Word,
    SuffixAdj,
    Suffix,
}

#[derive(Debug, Clone)]
pub enum Part {
    Literal(String),
    Slot(Slot),
}

/// Parse a template string with "{prefix}", "{word}", "{suffix_adj}", "{suffix}" placeholders.
/// Anything else is treated as a literal.
/// Returns an error for unknown placeholders like "{foo}".
pub fn parse_template(input: &str) -> anyhow::Result<Vec<Part>> {
    let mut parts = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current = String::new();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            // Save any accumulated literal
            if !current.is_empty() {
                parts.push(Part::Literal(current.clone()));
                current.clear();
            }

            // Parse the placeholder
            let mut placeholder = String::new();
            while let Some(next) = chars.next() {
                if next == '}' {
                    break;
                }
                placeholder.push(next);
            }

            let slot = match placeholder.as_str() {
                "prefix" => Slot::Prefix,
                "word" => Slot::Word,
                "suffix_adj" => Slot::SuffixAdj,
                "suffix" => Slot::Suffix,
                _ => {
                    return Err(anyhow::anyhow!(
                        "Unknown placeholder: {{{}}}",
                        placeholder
                    ))
                }
            };

            parts.push(Part::Slot(slot));
        } else {
            current.push(ch);
        }
    }

    // Save any remaining literal
    if !current.is_empty() {
        parts.push(Part::Literal(current));
    }

    Ok(parts)
}

/// Pick a random item from a list using the RNG, returning None if empty.
fn pick<'a>(list: &'a [String], rng: &mut SeededRng) -> Option<&'a str> {
    rng.gen_index(list.len()).and_then(|idx| {
        list.get(idx).map(|s| s.as_str())
    })
}

/// Join non-empty tokens with a separator.
fn join_tokens<'a>(tokens: impl IntoIterator<Item = &'a str>, separator: &str) -> String {
    tokens
        .into_iter()
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(separator)
}

pub struct Generator<'a> {
    config: &'a Config,
    words: WordLists,
    template: Option<Vec<Part>>,
}

impl<'a> Generator<'a> {
    /// Create a new generator without a template (default mode).
    pub fn new(config: &'a Config, words: WordLists) -> Self {
        Self {
            config,
            words,
            template: None,
        }
    }

    /// Create a new generator with a parsed template.
    pub fn with_template(
        config: &'a Config,
        words: WordLists,
        template: &str,
    ) -> anyhow::Result<Self> {
        let parsed = parse_template(template)?;
        Ok(Self {
            config,
            words,
            template: Some(parsed),
        })
    }

    /// Generate one name. Returns None if:
    /// - Default mode: the word pool is empty
    /// - Template mode: all slots rendered to empty strings AND there's at least one slot
    pub fn generate_one(&self, rng: &mut SeededRng) -> Option<String> {
        if let Some(ref template_parts) = self.template {
            self.generate_from_template(template_parts, rng)
        } else {
            self.generate_default(rng)
        }
    }

    /// Generate a unique name, retrying up to max_attempts times.
    /// Returns None if unable to generate a unique name after max_attempts.
    pub fn generate_unique(
        &self,
        rng: &mut SeededRng,
        used: &mut HashSet<String>,
        max_attempts: usize,
    ) -> Option<String> {
        for _ in 0..max_attempts {
            if let Some(name) = self.generate_one(rng) {
                if used.insert(name.clone()) {
                    return Some(name);
                }
            } else {
                // generate_one returned None, can't continue
                return None;
            }
        }
        None
    }

    /// Generate a name from template by rendering each part.
    /// Whitespace is normalized afterwards, so empty slots do not leave
    /// leading/trailing or doubled spaces behind.
    fn generate_from_template(&self, template_parts: &[Part], rng: &mut SeededRng) -> Option<String> {
        let mut result = String::new();
        let mut has_slot = false;
        let mut all_slots_empty = true;

        for part in template_parts {
            match part {
                Part::Literal(s) => result.push_str(s),
                Part::Slot(slot) => {
                    has_slot = true;
                    if let Some(word) = self.pick_for_slot(slot, rng) {
                        result.push_str(&word);
                        all_slots_empty = false;
                    }
                }
            }
        }

        // Return None if all slots were empty AND there was at least one slot
        if has_slot && all_slots_empty {
            None
        } else {
            Some(join_tokens(result.split_whitespace(), " "))
        }
    }

    /// Pick a word for a given slot.
    fn pick_for_slot(&self, slot: &Slot, rng: &mut SeededRng) -> Option<String> {
        let list = match slot {
            Slot::Prefix => &self.words.prefixes,
            Slot::Word => &self.words.words,
            Slot::SuffixAdj => &self.words.suffix_adjs,
            Slot::Suffix => &self.words.suffix_names,
        };
        pick(list, rng).map(String::from)
    }

    /// Generate a name in default mode: build token list, then join with separator.
    fn generate_default(&self, rng: &mut SeededRng) -> Option<String> {
        let g = &self.config.generator;
        let mut tokens: Vec<&str> = Vec::new();

        if rng.gen_bool(g.prefix_probability) {
            if let Some(p) = pick(&self.words.prefixes, rng) {
                if rng.gen_bool(g.prefix_article_probability) {
                    tokens.push("The");
                }
                tokens.push(p);
            }
        }

        let word = pick(&self.words.words, rng)?;
        tokens.push(word);

        if rng.gen_bool(g.suffix_name_probability) {
            if let Some(sn) = pick(&self.words.suffix_names, rng) {
                tokens.push(&g.fillword);
                if rng.gen_bool(g.suffix_article_probability) {
                    tokens.push("the");
                }
                if rng.gen_bool(g.suffix_adjectiv_probability) {
                    if let Some(sa) = pick(&self.words.suffix_adjs, rng) {
                        tokens.push(sa);
                    }
                }
                tokens.push(sn);
            }
        }

        Some(join_tokens(tokens, &g.separator))
    }
}
