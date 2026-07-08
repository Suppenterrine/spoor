use spoor::db::WordRecord;
use spoor::lookup;

#[test]
fn tokenize_removes_stopwords_and_lowercases() {
    let query = "Eine CLI für die Synchronisation von Logs";
    let tokens = lookup::tokenize(query);

    // Should remove German/English stopwords: Eine, für, die, von
    // Should keep: CLI, Synchronisation, Logs
    assert!(!tokens.contains(&"eine".to_string()));
    assert!(!tokens.contains(&"für".to_string()));
    assert!(!tokens.contains(&"die".to_string()));
    assert!(!tokens.contains(&"von".to_string()));

    // Should contain lowercased tokens
    assert!(tokens.contains(&"cli".to_string()));
    assert!(tokens.contains(&"synchronisation".to_string()));
    assert!(tokens.contains(&"logs".to_string()));

    // Deduplication should be preserved
    assert_eq!(tokens.len(), 3);
}

#[test]
fn rank_precedence_word_hit_beats_tag_hit_beats_etymology_hit() {
    // Create three records where each token hits differently
    let records = vec![
        WordRecord {
            id: "en_zenith".to_string(),
            word: "zenith".to_string(),
            word_class: Some("noun".to_string()),
            language: Some("en".to_string()),
            system: Some("nature".to_string()),
            tags: Some("height,peak".to_string()),
            seed_weight: 1.0,
            source: Some("wiki".to_string()),
            etymology: Some("from Arabic as-summa".to_string()),
            origin_lang: Some("ar".to_string()),
        },
        WordRecord {
            id: "en_peak".to_string(),
            word: "peak".to_string(),
            word_class: Some("noun".to_string()),
            language: Some("en".to_string()),
            system: Some("nature".to_string()),
            tags: Some("zenith,height".to_string()),
            seed_weight: 1.0,
            source: Some("wiki".to_string()),
            etymology: Some("of uncertain origin".to_string()),
            origin_lang: None,
        },
        WordRecord {
            id: "en_summit".to_string(),
            word: "summit".to_string(),
            word_class: Some("noun".to_string()),
            language: Some("en".to_string()),
            system: Some("nature".to_string()),
            tags: Some("top,high".to_string()),
            seed_weight: 1.0,
            source: Some("wiki".to_string()),
            etymology: Some("related to zenith, from Latin summit point".to_string()),
            origin_lang: Some("la".to_string()),
        },
    ];

    let matches = lookup::rank(&records, "zenith");

    // zenith hits "word" in first record (5.0 * 1.0) = 5.0
    // zenith hits "tag" in second record (3.0 * 1.0) = 3.0
    // zenith hits "etymology" in third record (1.0 * 1.0) = 1.0

    assert_eq!(matches.len(), 3);
    assert_eq!(matches[0].record.word, "zenith");
    assert_eq!(matches[1].record.word, "peak");
    assert_eq!(matches[2].record.word, "summit");
}

#[test]
fn rank_is_deterministic_with_shuffled_input() {
    let record1 = WordRecord {
        id: "en_alpha".to_string(),
        word: "alpha".to_string(),
        word_class: Some("noun".to_string()),
        language: Some("en".to_string()),
        system: Some("test".to_string()),
        tags: Some("first,beginning".to_string()),
        seed_weight: 1.0,
        source: None,
        etymology: Some("Greek letter".to_string()),
        origin_lang: Some("grc".to_string()),
    };

    let record2 = WordRecord {
        id: "en_beta".to_string(),
        word: "beta".to_string(),
        word_class: Some("noun".to_string()),
        language: Some("en".to_string()),
        system: Some("test".to_string()),
        tags: Some("second,beta".to_string()),
        seed_weight: 1.0,
        source: None,
        etymology: Some("Greek letter".to_string()),
        origin_lang: Some("grc".to_string()),
    };

    let records1 = vec![record1.clone(), record2.clone()];
    let records2 = vec![record2, record1];

    let matches1 = lookup::rank(&records1, "letter");
    let matches2 = lookup::rank(&records2, "letter");

    // Both should match both records
    assert_eq!(matches1.len(), 2);
    assert_eq!(matches2.len(), 2);

    // The order should be identical despite input being shuffled
    assert_eq!(matches1[0].record.word, matches2[0].record.word);
    assert_eq!(matches1[1].record.word, matches2[1].record.word);
}

#[test]
fn rank_tiebreak_by_seed_weight() {
    let record_low_weight = WordRecord {
        id: "en_a".to_string(),
        word: "alpha".to_string(),
        word_class: Some("noun".to_string()),
        language: Some("en".to_string()),
        system: Some("test".to_string()),
        tags: Some("test".to_string()),
        seed_weight: 1.0,
        source: None,
        etymology: None,
        origin_lang: None,
    };

    let record_high_weight = WordRecord {
        id: "en_b".to_string(),
        word: "beta".to_string(),
        word_class: Some("noun".to_string()),
        language: Some("en".to_string()),
        system: Some("test".to_string()),
        tags: Some("test".to_string()),
        seed_weight: 2.0,
        source: None,
        etymology: None,
        origin_lang: None,
    };

    let records = vec![record_low_weight, record_high_weight];

    // Both records have the same hits (tag 3.0 + system 2.0 = 5.0), but different seed_weights
    let matches = lookup::rank(&records, "test");

    assert_eq!(matches.len(), 2);
    // Higher seed_weight should come first
    assert_eq!(matches[0].record.word, "beta");
    assert_eq!(matches[0].score, 10.0); // (3.0 + 2.0) * 2.0 = 5.0 * 2.0
    assert_eq!(matches[1].record.word, "alpha");
    assert_eq!(matches[1].score, 5.0); // (3.0 + 2.0) * 1.0 = 5.0 * 1.0
}

#[test]
fn explain_format() {
    let m = lookup::Match {
        record: WordRecord {
            id: "la_zeus".to_string(),
            word: "zeus".to_string(),
            word_class: Some("proper".to_string()),
            language: Some("la".to_string()),
            system: Some("myth_greek".to_string()),
            tags: Some("sky,thunder,king".to_string()),
            seed_weight: 1.2,
            source: Some("curated".to_string()),
            etymology: Some("griech. Zeus, idg. *dyeus 'Himmel, Tag'".to_string()),
            origin_lang: Some("grc".to_string()),
        },
        score: 5.0,
        matched: vec!["sky (tag)".to_string(), "thunder (tag)".to_string()],
    };

    let explanation = lookup::explain(&m);

    // Should contain word
    assert!(explanation.contains("zeus"));
    // Should contain etymology
    assert!(explanation.contains("griech. Zeus"));
    // Should contain origin_lang
    assert!(explanation.contains("grc"));
    // Should contain system
    assert!(explanation.contains("myth_greek"));
    // Should contain matched items
    assert!(explanation.contains("sky (tag)"));
    assert!(explanation.contains("thunder (tag)"));
    // Should have German format
    assert!(explanation.contains("Treffer:"));
}

#[test]
fn no_match_returns_empty() {
    let records = vec![WordRecord {
        id: "en_test".to_string(),
        word: "test".to_string(),
        word_class: None,
        language: None,
        system: None,
        tags: None,
        seed_weight: 1.0,
        source: None,
        etymology: None,
        origin_lang: None,
    }];

    let matches = lookup::rank(&records, "xyzzy quux nonexistent");

    assert_eq!(matches.len(), 0);
}
