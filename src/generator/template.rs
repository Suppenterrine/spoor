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

#[derive(Debug, Clone)]
pub struct GeneratedName {
    pub text: String,
}

pub struct Generator<'a> {
    pub config: &'a Config,
    pub words: WordLists,
}

impl<'a> Generator<'a> {
    pub fn new(config: &'a Config, words: WordLists) -> Self {
        Self { config, words }
    }

    pub fn generate_one(&self, rng: &mut SeededRng, used: &mut HashSet<String>) -> String {
        let separator = self.config.generator.separator.as_str();
        let fillword = self.config.generator.fillword.as_str();
        let prefix_prob = self.config.generator.prefix_probability;
        let prefix_article_prob = self.config.generator.prefix_article_probability;
        let suffix_prob = self.config.generator.suffix_name_probability;
        let suffix_adj_prob = self.config.generator.suffix_adjectiv_probability;
        let suffix_article_prob = self.config.generator.suffix_article_probability;

        loop {
            let mut prefix = String::new();
            if let Some(p) = self
                .words
                .prefixes
                .get(rng.gen_index(self.words.prefixes.len()).unwrap_or(0))
            {
                if rng.gen_bool(prefix_article_prob) {
                    prefix = format!("The{}{}", separator, p);
                } else {
                    prefix = p.clone();
                }
            }

            let mut name = String::new();
            if let Some(n) = self
                .words
                .words
                .get(rng.gen_index(self.words.words.len()).unwrap_or(0))
            {
                name = n.clone();
            }

            let mut suffix_adjective = String::new();
            if let Some(sa) = self
                .words
                .suffix_adjs
                .get(rng.gen_index(self.words.suffix_adjs.len()).unwrap_or(0))
            {
                suffix_adjective = sa.clone();
            }

            let mut suffix_name = String::new();
            if let Some(sn) = self
                .words
                .suffix_names
                .get(rng.gen_index(self.words.suffix_names.len()).unwrap_or(0))
            {
                suffix_name = sn.clone();
            }

            let mut result = String::new();
            if rng.gen_bool(prefix_prob) {
                result = format!("{}{}{}", prefix, separator, name);
            } else {
                result = name;
            }

            if rng.gen_bool(suffix_prob) {
                if rng.gen_bool(suffix_adj_prob) {
                    result = format!(
                        "{}{}{}{}{}{}{}",
                        result, separator, fillword, separator, suffix_adjective, separator, suffix_name
                    );
                } else {
                    result =
                        format!("{}{}{}{}{}", result, separator, fillword, separator, suffix_name);
                }
            }

            if result.contains("of") && rng.gen_bool(suffix_article_prob) {
                result = result.replacen("of", &format!("of{}the", separator), 1);
            }

            if used.insert(result.clone()) {
                return result;
            }
        }
    }
}
