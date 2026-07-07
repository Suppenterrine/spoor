# Project Brief — Name Generator

## Ziel

Ein CLI-Tool, das aus einem mehrsprachigen, thematischen Wortschatz zufällige Namen generiert — mit Seed-Steuerung, gewichteten Vorlagen und reproduzierbaren Runs.

Zweitziel: ein **Reverse-Lookup**, der aus einem Anwendungsfall/Beschreibung über semantische Suche passende Einzelbegriffe mit etymologischer Begründung vorschlägt — als Marken-/Namensfinder.

## North Star

Die eigentliche Wertschöpfung liegt nicht in der Zufallsgenerierung, sondern im **Reverse-Lookup**: einen Begriff finden, der einen Anwendungsfall *einzeln* und präzise benennt, mit nachvollziehbarem etymologischen Kern. Alles andere dient diesem Zweck.

## Aktueller Stand

- Node.js-Prototyp mit CSV-basierten Wortlisten (`csvData/`), Wahrscheinlichkeitskonfiguration (`config.json`), `inquirer`-Interaktivität.
- Generierung: `prefix + name + "of" + suffix_adjective + suffix_name`, mit Artikel-Platzierung und Duplikats-Prävention.
- Keine Tests, kein Seed, keine Datenbank, keine Semantik.

## Kriterien für die neue Implementierung

1. **Binary Delivery**: Ein einziges Binary, keine Node-Laufzeit, keine `node_modules`-Abhängigkeiten für Endnutzer.
2. **Reproduzierbarkeit**: Jede Generierung über ein explizites Seed steuer- und wiederholbar.
3. **Erweiterbare Wortdatenbank**: Neue Sprachen, Systeme (Mythologie, Natur, Technik) durch Import-Pipelines, nicht durch manuelles CSV-Pflegen.
4. **Reverse-Lookup-First**: Der Anwendungsfall "Finde einen Namen für X" steht über dem Zufallsgenerator.

## Datenquellen

### Primär: Wiktionary-Dump + `wiktextract`

- **Was**: `wiktextract` (Python-Package) parst Wiktionary-XML-Dumps in strukturiertes JSON (Lemmata, Wortart, Etymologie, Übersetzungen).
- **Sprachen**: EN, DE, LA (Latein), GR (Altgriechisch), ohne massive Filter bereits brauchbare 6- bis 7-stellige Kandidatenmengen.
- **Qualität**: Gut für Substantive, Eigennamen-Kandidaten, Lehnwörter. Weniger gut für Fiktion/Erfundenes — das ist okay, das bauen wir später über Curated-Wordlists drauf.
- **Pipeline**: Einmalig einen Dump parsen → JSON extrahieren → in die lokale Datenbank indexen. Bei Bedarf inkrementell aktualisieren.

### Sekundär: Thematische Curated-Wordlists

Quellen für Systeme wie Mythologie, Natur, Technik:
- Offene GitHub-Wordlists (`kkrypt0nn/wordlists` u.ä.)
- Scraping von Seiten mit thematischen Glossaren (Greek, Norse, Egyptian mythology vocab).
- Eigenes Curation-Layer: jede Quelle bekommt einen `source_id`, Einträge werden dedupliziert und mit `system`-Tag versehen.

### Datenbankschema (Vorschlag)

Tabelle `words`:
```
id             TEXT PRIMARY KEY    -- "en_apple", "la_aurum", "myth_zeus"
word           TEXT                -- Oberflächenform
language       TEXT                -- "en", "de", "la", "gr", ...
system         TEXT[]              -- ["greek_myth", "norse_myth", "nature", ...]
word_class     TEXT                -- "noun", "proper", "adj", ...
etymology      TEXT                -- Kurzbeschreibung Herkunft
origin_lang    TEXT                -- z. B. "grc", "non", ...
tags           TEXT[]              -- flexible Marker: ["fire", "sky", "craft"]
seed_weight    REAL DEFAULT 1.0    -- Template-Gewicht, beeinflussbar
source         TEXT                -- Herkunft der Aufnahme
```

Volltext-Suche: DuckDB FTS oder `sqlite-fts5` für schnelle Filter nach Sprache/System/Tags.

## Architektur

### Modular

```
namegen/
├── Cargo.toml
├── src/
│   ├── main.rs             # CLI (clap)
│   ├── db.rs               # Datenbank-Adapter
│   ├── import/
│   │   ├── wiktionary.rs   # wiktextract-JSON → DB
│   │   └── curated.rs      # CSV/JSON-Wordlists → DB
│   ├── gen/
│   │   ├── template.rs     # Template-Parser + -Auswerter
│   │   ├── seed.rs         # Seedable RNG, Komponenten-Gewichtung
│   │   └── render.rs       # Ausgabeformatierung (plain, markdown, json)
│   └── lookup/
│       ├── embed.rs        # lokale Embeddings (sentence-transformers viaORT?)
│       ├── cosine.rs       # Cosine-Query über Wortvektoren
│       └── explain.rs      # Etymologie-Begründung aus DB zusammenbauen
└── data/
    └── words.duckdb        # oder sqlite, je nach Entscheidung
```

### CLI-Oberfläche

**Generierung:**
```
namegen gen --seed 42 --systems "greek_myth,norse_myth,nature" --count 5
namegen gen --template "The {prefix} of {suffix}" --seed 7
```

**Reverse-Lookup:**
```
namegen find "a CLI tool to synchronize data between services"
namegen find "brand for a sleep tracking app" --count 3 --explain
namegen find "concept of layered reality" --systems "philosophy,latin"
```

## Technologieentscheidungen

| Bereich | Entscheidung | Begründung |
|---|---|---|
| Sprache | **Rust** | Binary Delivery, crates.io-Ökosystem, `clap` + `rand` + `rusqlite`/`duckdb` |
| Datenbank | **DuckDB** | Bessere ANALYTICS + FTS, columnar, embedded, Rust-Bindings vorhanden |
| Embeddings | **sentence-transformers (ONNX Runtime)** | Lokal lauffähig, keine Cloud, gute multilinguale Modelle |
| CLI Framework | **clap** | Etabliert, Subcommands, Auto-Completion, gut dokumentiert |
| Zufall | **rand + SeedableRng** | Deterministisch, Seed als Kommando-Arg, reproduzierbar |
| Importer | **Python + `wiktextract`** | Einmalige Pipeline, schneller als Rust-XML-Parsing für 20 GB Dumps |

## Risiken

- **Embeddings lokal**: Große Modelle → großes Binary. Für Early Adopter reicht ein kleineres Modell oder Keyword-Fallback.
- **Wiktionary-Qualität**: Nicht alle Sprachen gleich gut. DE/Gr/La sind okay; exotischere müssen ggf. separat kuratiert werden.
- **Token-Budget bei Embeddings**: Multi-Token-Begriffe müssen pooled werden; einfache Lösung: je Wort einzeln embedden, dann über Gesamt-Ähnlichkeit aggregieren.

## Phasen

### Phase 0 — Setup & Proof of Concept (1–2 Wochen)
- Rust-Projekt-Skelett mit `clap` + `rusqlite`.
- Lokale Datenbank mit 50 handverlesenen Testwörtern (DE/EN/LA).
- `namegen gen --seed 1` gibt reproduzierbar Wörter aus.
- Templates funktionieren mit Platzhaltern.

### Phase 1 — Datenbank-Basis (2–3 Wochen)
- Wiktionary-Pipeline (Python/`wiktextract`) für DE + EN + Latin.
- Import-Script → DuckDB.
- Erste größere Generierung läuft stabil mit echten Datenmengen.

### Phase 2 — Generative Engine (1 Woche)
- Seed-Steuerung, gewichtete Komponenten.
- Template-Konfiguration über CLI-Flags und Config-Datei.
- `--count`, `--format json`, Ausgabe an Clipboard.

### Phase 3 — Reverse Lookup (2–3 Wochen)
- Embedding-Modell einbinden, Vektoren indexen.
- Cosine-Query + Begründungsgenerator aus Etymologie-Feldern.
- `--count`, `--explain`, Ergebnis-Filterung nach System.

### Phase 4 — Curated Systems (1–2 Wochen)
- Scraper für griechische/nordische/ägyptische Mythologie-Wortlisten.
- Dedup + Tagging → Datenbank.
- `--systems`-Flag erweitern.

### Phase 5 — Polish & Release (1 Woche)
- Binary-Build getestet auf Windows/macOS/Linux.
- README mit Beispielen, Screenshots.
- GitHub Release + Changelog.

## Offene Fragen

1. Soll ein *einziger* Begriff pro Generierung bevorzugt werden, oder immer mehrere zur Auswahl? → Konfigurierbar.
2. Welche Embedding-Größe ist akzeptabel fürs Binary? → Erfahrungswert nach Modell-Eval abhängig.
3. Soll die Datenbank mitgeliefert werden (~50–500 MB) oder bei Bedarf heruntergeladen? → Hybrid: Basis eingebettet, Erweiterungen per Download.
