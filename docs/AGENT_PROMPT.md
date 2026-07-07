# Agent Prompt — Name Generator Phase 0

## Deine Aufgabe

Du baust Phase 0 des **Name Generator** Projekts: eine Rust-Neufassung des vorhandenen Node.js-Prototyps mit reproduzierbarer Seed-Generierung, gewichteten Templates und einer lokalen Wortdatenbank.

## Kontext

Das Projekt liegt in `~/Documents/Repos/Name-Generator/`.

### Was jetzt existiert
- Node.js-Prototyp (`app.js`, `package.json`, `config.json`, `csvData/`)
- Generierung zufälliger Namen aus CSV-Wortlisten mit Wahrscheinlichkeitskonfiguration
- Template-Aufbau: `prefix + name + "of" + suffix_adjective + suffix_name` mit optionalen Artikeln und Duplikats-Prävention
- Keine Tests, kein Seed, keine Datenbank, keine Semantik

### Was wir wollen
Siehe `docs/PROJECT_BRIEF.md` und `docs/NORTH_STAR.md`. Kurzfassung:

- **Binary Delivery**: Rust, `cargo build --release`, ein einziges Binary
- **Reproduzierbarkeit**: Generierung über explizites `--seed` steuer- und wiederholbar
- **Erweiterbare Wortdatenbank**: Import aus CSV/JSON, später Wiktionary-Dump + kuratierte Listen
- **Reverse-Lookup später**: In Phase 3. Phase 0 muss das nicht können, aber die Datenbank-Struktur sollte es später erlauben.

## Was du jetzt tun sollst

### 1. Inspiziere den Bestand

Lies folgende Dateien vollständig:
- `app.js` — die gesamte aktuelle Logik
- `package.json` — Abhängigkeiten
- `config.json` — Standard-Konfiguration
- `csvData/*.csv` — damit du das Datenformat verstehst

Verstehe die genaue Funktionsweise der Namensgenerierung, insbesondere:
- Welche Platzhalter/Template-Teile es gibt
- Wie Wahrscheinlichkeiten angewendet werden
- Wie Duplikats-Prävention funktioniert
- Welche CLI-Interaktivität existiert (`inquirer`)

### 2. Errichte das Rust-Grundgerüst

```
name-generator/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── cli.rs
│   ├── config.rs
│   ├── generator/
│   │   ├── mod.rs
│   │   ├── template.rs
│   │   └── rng.rs
│   └── db/
│       └── mod.rs
├── data/
│   └── words.csv          # Start-Datensatz, mindestens 50 Einträge
└── tests/
```

**Auszuführende Schritte im Terminal:**

```bash
cd ~/Documents/Repos/Name-Generator
cargo init --name name-generator
```

Abhängigkeiten in `Cargo.toml`:
```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
csv = "1"
rand = "0.8"
rand_chacha = "0.3"
rusqlite = { version = "0.31", features = ["bundled"] }
toml = "0.8"
```

### 3. Implementiere Phase 0

#### A) Datenbankschicht (`src/db/mod.rs`)
- SQLite-Datenbank unter `data/words.db`
- Tabelle `words` mit Spalten:
  - `id TEXT PRIMARY KEY`
  - `word TEXT NOT NULL`
  - `word_class TEXT` — "noun", "proper", "adj"
  - `language TEXT` — "en", "de", "la", ...
  - `system TEXT` — thematischer Tag, z.B. "nature", "myth_greek"
  - `tags TEXT` — kommaseparierte flexible Tags
  - `seed_weight REAL DEFAULT 1.0`
  - `source TEXT`
- Funktionen: `open()`, `insert_words()`, `get_random_by_system()`, `search_by_tag()`
- Implementiere einen **Importer**, der `data/words.csv` (oder beliebige CSV/JSON) einliest und in die DB schreibt — mit Dedup auf `(word, language, system)`.

#### B) Template-Engine (`src/generator/`)
Erhalte die jetzige Logik aus `app.js`, aber als Rust-Code:
- Platzhalter: `{prefix}`, `{word}`, `{suffix_adj}`, `{suffix}`
- Artikel-Regeln: `prefix_article_probability`, `suffix_article_probability`
- Separator konfigurierbar (Standard: Leerzeichen)
- Fillword konfigurierbar (Standard: "of")
- Duplikats-Prävention: innerhalb eines Runs generierte Namen nicht wiederholen

#### C) Seed-Steuerung (`src/generator/rng.rs`)
- `rand_chacha::ChaCha8Rng` mit `SeedableRng::seed_from_u64(seed)`
- CLI-Flag `--seed <u64>` — ohne `--seed` wird ein zufälliger Seed erzeugt und in der Ausgabe angezeigt
- Alle Zufallsentscheidungen laufen über diesen RNG

#### D) CLI (`src/cli.rs`, `main.rs`)
 Nutze `clap` mit Subcommands:

```bash
# Generierung
name-generator gen --seed 42 --count 5 --systems "nature,myth_greek"
name-generator gen --template "The {prefix} of {suffix}" --seed 7

# Import
name-generator import data/words.csv

# Datenbank-Info
name-generator info
```

- `gen`:
  - `--seed <u64>` — optional
  - `--count <n>` — Anzahl Ergebnisse, Standard 1
  - `--systems <csv>` — Filter nach System-Tags
  - `--template <str>` — optionales Template
  - `--format <text|json>` — Ausgabeformat
  - `--config <path>` — alternativer Pfad zur Config-Datei (Standard: `config.toml`)

- `info`:
  - Gibt Statistiken aus: Gesamtzahl Wörter, aufgeschlüsselt nach Sprache und System

**Achtung**: Wenn `--seed` weggelassen wird, erzeuge einen zufälligen Seed, gib ihn **in der Ausgabe** aus, und verwende ihn für diesen Run. Dadurch ist jeder Run reproduzierbar, sobald der Seed bekannt ist.

#### E) Konfiguration (`src/config.rs`)
- Liest `config.toml` (Standardpfad: `./config.toml`)
- Struktur:
```toml
[generator]
prefix_article_probability = 0.2
prefix_probability = 0.8
suffix_article_probability = 0.3
suffix_adjectiv_probability = 0.5
suffix_name_probability = 0.5
separator = " "
fillword = "of"

[db]
path = "data/words.db"
```
- CLI-Flags überschreiben Config-Werte.

#### F) Start-Datensatz
Erstelle `data/words.csv` mit mindestens 50 Einträgen, gemischt aus:
- Englische Substantive (Natur, Himmelskörper, Handwerk)
- Deutsche Substantive
- Lateinische Begriffe
- Griechische Mythologie-Eigennamen

Format: `word,language,word_class,system,tags,seed_weight,source`

Beispiel:
```csv
word,language,word_class,system,tags,seed_weight,source
silvan,la,noun,nature,"forest,tree",1.0,wiktionary
oak,en,noun,nature,"tree,strength",1.0,wiktionary
zeus,la,proper,myth_greek,"sky,thunder,king",1.2,curated
```

#### G) Tests
- Ein Integrationstest, der mit Seed `12345` einen Namen generiert und auf Gleichheit mit einem gespeicherten Erwartungswert prüft.
- Ein Import-Test: CSV importieren, dann Anzahl Zeilen in DB prüfen.

```bash
cargo test
```

### 4. Qualitätskriterien

- `cargo build --release` kompiliert fehlerfrei unter Windows, macOS und Linux.
- `cargo test` läuft grün.
- Binary ist unter 15 MB.
- `name-generator gen --seed 12345 --count 3` liefert deterministisch dasselbe Ergebnis bei wiederholtem Aufruf.
- `name-generator import data/words.csv` importiert die CSV und `info` zeigt die korrekten Zählungen.
- README.md wird aktualisiert:
  - Build-Instruktionen
  - CLI-Beispiele
  - Hinweis zur späteren Reverse-Lookup-Phase

### 5. Was in Phase 0 NICHT gebaut wird

- Keine Embeddings
- Keine Cosine-Suche
- Keine Wiktionary-Dump-Pipeline (die folgt in Phase 1)
- Keine `--explain`-Ausgabe für Etymologie
- Keine thematischen Scraper für Mythologie-Seiten
- Keine TTS-Ausgabe, keine GUI

## Wichtige Hinweise

- Das vorhandene `app.js` ist der Anhaltspunkt für die Template-Logik, nicht das Endprodukt. Du kannst die Logik frei in idiomatisches Rust übertragen.
- `config.json` wird durch `config.toml` abgelöst.
- Die Wortdatenbank ist erstmal SQLite. Später kann sie durch DuckDB ersetzt werden, ohne die CLI-Oberfläche zu ändern — deswegen kommt die DB-Zugriffsschicht in ein eigenes Modul (`db/`).
- Der Reverse-Lookup in Phase 3 braucht semantische Vektoren. Deswegen: Speichere bei `import()` jede Wort-Zeile als einen eigenen record, **nicht** gruppiert. Später kommt pro Wort ein `embedding BLOB` dazu.

## Deliverable am Ende

1. Kompilierbares Rust-Projekt unter `~/Documents/Repos/Name-Generator/`
2. Binary unter `target/release/name-generator.exe` (Windows) bzw. `target/release/name-generator`
3. Aktualisiertes `README.md`
4. `data/words.csv` und `data/words.db` mit den Testdaten
5. Eine kurze Bestätigungsnachricht an mich, wenn `cargo test` und `cargo build --release` grün sind.
