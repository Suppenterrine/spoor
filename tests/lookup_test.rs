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
            translit: None,
            registers: None,
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
            translit: None,
            registers: None,
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
            translit: None,
            registers: None,
        },
    ];

    let matches = lookup::rank(&records, "zenith");

    // Anti-echo: the record "zenith" itself is excluded (the query word is
    // never the interesting answer). "peak" hits via gloss (IDF-weighted,
    // plus the bridge concepts from the zenith record), "summit" only via
    // etymology substring — so peak ranks above summit.
    assert_eq!(matches.len(), 2);
    assert!(matches.iter().all(|m| m.record.word != "zenith"), "echo must be filtered");
    assert_eq!(matches[0].record.word, "peak");
    assert_eq!(matches[1].record.word, "summit");
}

#[test]
fn rank_allows_echo_with_flag() {
    let records = vec![WordRecord {
        id: "en_zenith".to_string(),
        word: "zenith".to_string(),
        word_class: Some("noun".to_string()),
        language: Some("en".to_string()),
        system: Some("nature".to_string()),
        tags: Some("height,peak".to_string()),
        seed_weight: 1.0,
        source: None,
        etymology: None,
        origin_lang: None,
        translit: None,
        registers: None,
    }];

    let filtered = lookup::rank_semantic(&records, &records, "zenith", &[], &[], false);
    assert!(filtered.is_empty(), "echo filtered by default");

    let allowed = lookup::rank_semantic(&records, &records, "zenith", &[], &[], true);
    assert_eq!(allowed.len(), 1);
    assert_eq!(allowed[0].record.word, "zenith");
}

#[test]
fn rank_bridges_query_word_to_gloss_concepts() {
    // The German record "Baum" translates the query into the concept "tree";
    // the Latin record "arbor" is found over its gloss — and "Baum" itself
    // is excluded as echo. This is the core North-Star behavior.
    let records = vec![
        WordRecord {
            id: "de_Baum".to_string(),
            word: "Baum".to_string(),
            word_class: Some("noun".to_string()),
            language: Some("de".to_string()),
            system: Some("wiktionary_de".to_string()),
            tags: Some("tree".to_string()),
            seed_weight: 1.0,
            source: None,
            etymology: None,
            origin_lang: None,
            translit: None,
            registers: None,
        },
        WordRecord {
            id: "la_arbor".to_string(),
            word: "arbor".to_string(),
            word_class: Some("noun".to_string()),
            language: Some("la".to_string()),
            system: Some("wiktionary_la".to_string()),
            tags: Some("a tree,a mast".to_string()),
            seed_weight: 1.0,
            source: None,
            etymology: Some("from proto-italic *ardhos".to_string()),
            origin_lang: Some("la".to_string()),
            translit: None,
            registers: None,
        },
    ];

    let matches = lookup::rank(&records, "Baum");

    assert_eq!(matches.len(), 1, "only arbor should match; Baum is echo");
    assert_eq!(matches[0].record.word, "arbor");
    // The path must be visible for explain output
    assert!(
        matches[0].matched.iter().any(|m| m.contains("baum → tree")),
        "matched should carry the bridge path, got: {:?}",
        matches[0].matched
    );
}

#[test]
fn rank_association_edge_expands_query() {
    // Nexus edge "track --verwandt--> spoor": the query "track" finds the
    // record "spoor" over the association hop, with the path in matched.
    use spoor::db::Edge;

    let records = vec![WordRecord {
        id: "en_spoor".to_string(),
        word: "spoor".to_string(),
        word_class: Some("noun".to_string()),
        language: Some("en".to_string()),
        system: Some("wiktionary_en".to_string()),
        tags: Some("the trail of an animal".to_string()),
        seed_weight: 1.0,
        source: None,
        etymology: None,
        origin_lang: None,
        translit: None,
        registers: None,
    }];
    let edges = vec![Edge {
        src: "track".to_string(),
        rel: "verwandt".to_string(),
        dst: "spoor".to_string(),
        weight: 0.6,
        source: None,
    }];

    let without = lookup::rank_semantic(&records, &records, "track", &[], &[], false);
    assert!(without.is_empty(), "no hit without the edge");

    let with = lookup::rank_semantic(&records, &records, "track", &[], &edges, false);
    assert_eq!(with.len(), 1);
    assert_eq!(with[0].record.word, "spoor");
    assert!(
        with[0].matched.iter().any(|m| m.contains("track → spoor (verwandt)")),
        "matched should carry the association path, got: {:?}",
        with[0].matched
    );
}

#[test]
fn rank_near_echo_compound_is_dampened() {
    // "Apfelbaum" contains the query token "baum" → dampened below the
    // true association "arbor", even though both hit the "tree" bridge.
    let make = |id: &str, word: &str, lang: &str, tags: &str| WordRecord {
        id: id.to_string(),
        word: word.to_string(),
        word_class: Some("noun".to_string()),
        language: Some(lang.to_string()),
        system: Some("test".to_string()),
        tags: Some(tags.to_string()),
        seed_weight: 1.0,
        source: None,
        etymology: None,
        origin_lang: None,
        translit: None,
        registers: None,
    };
    let records = vec![
        make("de_Baum", "Baum", "de", "tree"),
        make("de_Apfelbaum", "Apfelbaum", "de", "apple tree"),
        make("en_arbor", "arbor", "en", "a tree"),
    ];

    let matches = lookup::rank(&records, "Baum");

    assert!(matches.iter().all(|m| m.record.word != "Baum"), "echo filtered");
    assert_eq!(matches[0].record.word, "arbor", "association beats compound");
    let apfel = matches.iter().find(|m| m.record.word == "Apfelbaum");
    assert!(apfel.is_some(), "near-echo stays available, only dampened");
    assert!(apfel.unwrap().score < matches[0].score);
}

#[test]
fn rank_register_boost_prefers_poetic_word() {
    let make = |id: &str, word: &str, registers: Option<&str>| WordRecord {
        id: id.to_string(),
        word: word.to_string(),
        word_class: Some("noun".to_string()),
        language: Some("en".to_string()),
        system: Some("test".to_string()),
        tags: Some("light".to_string()),
        seed_weight: 1.0,
        source: None,
        etymology: None,
        origin_lang: None,
        translit: None,
        registers: registers.map(str::to_string),
    };
    let records = vec![
        make("en_plain", "aglow", None),
        make("en_poetic", "lumen", Some("poetic")),
    ];

    let matches = lookup::rank(&records, "light");

    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].record.word, "lumen", "poetic register wins the tie");
}

#[test]
fn rank_origin_language_beats_query_language_on_equal_hits() {
    let make = |id: &str, word: &str, lang: &str| WordRecord {
        id: id.to_string(),
        word: word.to_string(),
        word_class: Some("noun".to_string()),
        language: Some(lang.to_string()),
        system: Some("test".to_string()),
        tags: Some("forest".to_string()),
        seed_weight: 1.0,
        source: None,
        etymology: None,
        origin_lang: None,
        translit: None,
        registers: None,
    };
    let records = vec![make("en_woods", "woods", "en"), make("la_silva", "silva", "la")];

    let matches = lookup::rank(&records, "forest");

    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].record.word, "silva", "Latin gets the origin bonus");
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
        translit: None,
        registers: None,
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
        translit: None,
        registers: None,
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
        translit: None,
        registers: None,
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
        translit: None,
        registers: None,
    };

    let records = vec![record_low_weight, record_high_weight];

    // Both records have identical hits (gloss + system), but different seed_weights
    let matches = lookup::rank(&records, "test");

    assert_eq!(matches.len(), 2);
    // Higher seed_weight should come first; the weight scales the score linearly
    assert_eq!(matches[0].record.word, "beta");
    assert_eq!(matches[1].record.word, "alpha");
    assert!(
        (matches[0].score - 2.0 * matches[1].score).abs() < 1e-9,
        "seed_weight 2.0 should double the score: {} vs {}",
        matches[0].score,
        matches[1].score
    );
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
            translit: None,
            registers: None,
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
    // Should have German format (the path label)
    assert!(explanation.contains("Spur:"));
}

#[test]
fn explain_display_uses_given_word_and_omits_missing_fields() {
    let m = lookup::Match {
        record: WordRecord {
            id: "grc_σοφία".to_string(),
            word: "σοφία".to_string(),
            word_class: Some("noun".to_string()),
            language: Some("grc".to_string()),
            system: Some("wiktionary_grc".to_string()),
            tags: Some("wisdom".to_string()),
            seed_weight: 1.0,
            source: None,
            etymology: None,
            origin_lang: None,
            translit: Some("sophía".to_string()),
            registers: None,
        },
        score: 1.0,
        matched: vec!["wisdom (glosse)".to_string()],
    };

    let explanation = lookup::explain_display(&m, "sophía");

    assert!(explanation.starts_with("sophía — "));
    assert!(explanation.contains("Spur: wisdom (glosse)"));
    // No "?" placeholders for missing etymology/origin
    assert!(!explanation.contains('?'));
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
        translit: None,
        registers: None,
    }];

    let matches = lookup::rank(&records, "xyzzy quux nonexistent");

    assert_eq!(matches.len(), 0);
}
