//! Rule-based transliteration to Latin script (Baustein E).
//!
//! Primary source for Latin forms is the `translit` column filled from the
//! kaikki `forms` romanizations at fetch time; this module is the
//! deterministic fallback for records without one, and provides diacritic
//! folding so romanized forms ("lógos") compare equal to plain ASCII query
//! tokens ("logos").

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// Remove all combining marks (accents, breathings, Hebrew niqqud) after NFD
/// decomposition. "lógos" → "logos", "ἀκακία" → "ακακια".
pub fn fold_diacritics(s: &str) -> String {
    s.nfd().filter(|c| !is_combining_mark(*c)).collect()
}

/// True if the string contains no characters outside basic Latin ranges
/// (letters with diacritics count as Latin).
pub fn is_latin_script(s: &str) -> bool {
    s.chars().all(|c| {
        !c.is_alphabetic()
            || c.is_ascii_alphabetic()
            || matches!(c, '\u{00C0}'..='\u{024F}') // Latin-1 Supplement .. Latin Extended-B
    })
}

/// Transliterate a Greek base character (combining marks already stripped).
/// `modern` switches the letters that differ between Ancient (grc) and
/// Modern (el) conventions: β=b/v, η=e/i, φ=ph/f, υ=y/i.
fn greek_char(c: char, modern: bool) -> Option<&'static str> {
    Some(match c {
        'α' => "a",
        'β' => if modern { "v" } else { "b" },
        'γ' => "g",
        'δ' => "d",
        'ε' => "e",
        'ζ' => "z",
        'η' => if modern { "i" } else { "e" },
        'θ' => "th",
        'ι' => "i",
        'κ' => "k",
        'λ' => "l",
        'μ' => "m",
        'ν' => "n",
        'ξ' => "x",
        'ο' => "o",
        'π' => "p",
        'ρ' => "r",
        'σ' | 'ς' => "s",
        'τ' => "t",
        'υ' => if modern { "i" } else { "y" },
        'φ' => if modern { "f" } else { "ph" },
        'χ' => "ch",
        'ψ' => "ps",
        'ω' => "o",
        _ => return None,
    })
}

/// Transliterate a Hebrew consonant (final forms map like their base forms).
/// Consonantal only — vowel points are combining marks and already stripped;
/// the kaikki romanization is preferred wherever available.
fn hebrew_char(c: char) -> Option<&'static str> {
    Some(match c {
        'א' | 'ע' => "",
        'ב' => "v",
        'ג' => "g",
        'ד' => "d",
        'ה' => "h",
        'ו' => "v",
        'ז' => "z",
        'ח' => "ch",
        'ט' => "t",
        'י' => "y",
        'כ' | 'ך' => "k",
        'ל' => "l",
        'מ' | 'ם' => "m",
        'נ' | 'ן' => "n",
        'ס' => "s",
        'פ' | 'ף' => "f",
        'צ' | 'ץ' => "ts",
        'ק' => "k",
        'ר' => "r",
        'ש' => "sh",
        'ת' => "t",
        _ => return None,
    })
}

/// Rule-based Latin transliteration of `word`. Returns None if the word is
/// already Latin script (nothing to do). Unknown non-Latin characters are
/// passed through unchanged. `language` selects the Greek convention
/// ("el" = modern, anything else = classical).
pub fn to_latin(word: &str, language: Option<&str>) -> Option<String> {
    if is_latin_script(word) {
        return None;
    }

    let modern_greek = language == Some("el");
    let folded = fold_diacritics(word);
    let mut out = String::with_capacity(folded.len());

    for c in folded.chars() {
        let lower: char = c.to_lowercase().next().unwrap_or(c);
        if let Some(mapped) = greek_char(lower, modern_greek) {
            out.push_str(mapped);
        } else if let Some(mapped) = hebrew_char(lower) {
            out.push_str(mapped);
        } else {
            out.push(c);
        }
    }

    Some(out)
}

/// The Latin display form of a word: stored romanization first (folded to
/// the same case conventions it came with), rule-based fallback second,
/// the word itself if it is already Latin.
pub fn latin_form(word: &str, translit: Option<&str>, language: Option<&str>) -> String {
    if let Some(t) = translit {
        if !t.trim().is_empty() {
            return t.trim().to_string();
        }
    }
    to_latin(word, language).unwrap_or_else(|| word.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_strips_accents_and_breathings() {
        assert_eq!(fold_diacritics("lógos"), "logos");
        assert_eq!(fold_diacritics("ἀκακία"), "ακακια");
        assert_eq!(fold_diacritics("für"), "fur");
    }

    #[test]
    fn latin_words_pass_through() {
        assert_eq!(to_latin("arbor", Some("la")), None);
        assert_eq!(to_latin("Baum", Some("de")), None);
        assert_eq!(latin_form("arbor", None, Some("la")), "arbor");
    }

    #[test]
    fn ancient_greek_classical_convention() {
        assert_eq!(to_latin("σοφία", Some("grc")), Some("sophia".to_string()));
        assert_eq!(to_latin("λόγος", Some("grc")), Some("logos".to_string()));
        assert_eq!(to_latin("ψυχή", Some("grc")), Some("psyche".to_string()));
    }

    #[test]
    fn modern_greek_convention() {
        // β=v, η=i in Modern Greek
        assert_eq!(to_latin("βήτα", Some("el")), Some("vita".to_string()));
    }

    #[test]
    fn hebrew_consonantal_fallback() {
        assert_eq!(to_latin("שלום", Some("he")), Some("shlvm".to_string()));
    }

    #[test]
    fn stored_translit_wins_over_rules() {
        assert_eq!(latin_form("שלום", Some("shalom"), Some("he")), "shalom");
        assert_eq!(latin_form("σοφία", Some("sophía"), Some("grc")), "sophía");
    }
}
