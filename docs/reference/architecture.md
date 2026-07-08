# Architektur — Name Generator

Übersicht der modularen Architektur und der Datenflüsse.

## Modulstruktur

```
src/
├── main.rs              # Einstiegspunkt, delegiert an CLI
├── lib.rs               # Re-exports öffentlicher Schnittstellen
├── cli.rs               # Kommandozeileninterface (clap)
├── config.rs            # Konfiguration (TOML-Parsing)
├── db/mod.rs            # Datenbankoperationen (SQLite)
├── generator/
│   ├── mod.rs           # Öffentliche Exports
│   ├── rng.rs           # SeededRng (ChaCha8-Wrapper)
│   └── template.rs      # Template-Parser und Generator
└── csv_import           # (intern in cli.rs)
```

## Datenfluss

### Generierungs-Pipeline

```
config.toml
    ↓
Config::read() → Config { generator, db }
    ↓
Db::open(path) → SQLite-Verbindung
    ↓
db.words_by_class() → Vec<(word, word_class)>
    ↓
load_wordlists() → WordLists { prefixes, words, suffix_adjs, suffix_names }
    ↓
Generator::new(config, words) oder Generator::with_template(config, words, template_str)
    ↓
SeededRng::new(seed) oder rand::random()
    ↓
Generator::generate_unique(rng, used_set) → Wort
    ↓
Ausgabe (text oder json)
```

### Import-Pipeline

```
data/words.csv
    ↓
csv_import::read_words() → Vec<WordRecord>
    ↓
db.insert_words() → SQLite INSERT/REPLACE
    ↓
words.db (aktualisiert)
```

## Kern-Module

### 1. **config.rs** — Konfiguration

Liest `config.toml` und stellt Wahrscheinlichkeiten + Datenbankpfad bereit.

**Struktur**:
```rust
pub struct GeneratorConfig {
    pub prefix_article_probability: f64,
    pub prefix_probability: f64,
    pub suffix_article_probability: f64,
    pub suffix_adjectiv_probability: f64,
    pub suffix_name_probability: f64,
    pub separator: String,
    pub fillword: String,
}

pub struct Config {
    pub generator: GeneratorConfig,
    pub db: DbConfig,
}
```

**Verantwortung**:
- TOML-Datei parsen
- Konfigurationswerte bereitstellen
- Fehlerbehandlung bei fehlenden Dateien

---

### 2. **db/mod.rs** — Datenbankoperationen

Alle SQL-Operationen sind hier konzentriert. Keine SQL in anderen Modulen.

**Struktur**:
```rust
pub struct WordRecord {
    pub id: String,                    // language_word (Duplikat-Key)
    pub word: String,
    pub word_class: Option<String>,    // prefix, noun, proper, adj, suffix
    pub language: Option<String>,      // en, de, la, ...
    pub system: Option<String>,        // nature, myth_greek, craft, ...
    pub tags: Option<String>,          // Tags (z. B. fire,sky)
    pub seed_weight: f64,              // Gewicht (zukünftig)
    pub source: Option<String>,        // wiktionary, curated, ...
}

pub struct Db {
    conn: Connection,
}

pub struct DbStats {
    pub total: usize,
    pub by_language: Vec<(String, usize)>,
    pub by_system: Vec<(String, usize)>,
}
```

**Verantwortung**:
- SQLite-Verbindung verwalten
- Schema erstellen (`ensure_schema`)
- Wörter einfügen/ersetzen (`insert_words`)
- Abfragen: `list_systems()`, `list_languages()`, `list_classes()`, `list_words()`, `words_by_class()`, `stats()`
- Alle SQL-Transaktionen (Atomarität)

**Duplikat-Vermeidung**:
- Primary Key ist `id = language + "_" + word`
- `INSERT OR REPLACE` überschreibt bei Konflikt
- Beispiel: `de_wald` und `en_forest` sind verschiedene Einträge, aber `en_forest` (Duplikat) wird ersetzt

---

### 3. **generator/rng.rs** — Geseedete Randomisierung

Wrapper um `rand_chacha::ChaCha8Rng` für deterministische Randomisierung.

**Struktur**:
```rust
pub struct SeededRng {
    inner: ChaCha8Rng,  // ChaCha8-Algorithmus (schnell, deterministic)
}

impl SeededRng {
    pub fn new(seed: u64) -> Self { ... }
    pub fn gen_bool(&mut self, probability: f64) -> bool { ... }
    pub fn gen_index(&mut self, len: usize) -> Option<usize> { ... }
}
```

**Prinzipien**:
- Alle Randomisierung läuft über diesen RNG
- ChaCha8 ist schnell und kryptographisch sicher (nicht erforderlich, aber sicher)
- Seed bestimmt vollständig die Ausgabe: `f(config, words, seed)` → identische Ausgabe
- Nie `rand::random()` direkt verwenden
- Nie `SQL RANDOM()` verwenden (würde Determinismus brechen)

**Einsatz**:
- `gen_bool(probability)` → wählt Tokens aus (Wahrscheinlichkeit)
- `gen_index(len)` → wählt Wort aus Liste

---

### 4. **generator/template.rs** — Template und Generierung

Parst Templates, wählt Wörter und generiert Namen.

**Datentypen**:
```rust
pub struct WordLists {
    pub prefixes: Vec<String>,
    pub words: Vec<String>,
    pub suffix_adjs: Vec<String>,
    pub suffix_names: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum Slot {
    Prefix,      // {prefix}
    Word,        // {word}
    SuffixAdj,   // {suffix_adj}
    Suffix,      // {suffix}
}

pub enum Part {
    Literal(String),  // Literaler Text
    Slot(Slot),       // Platzhalter
}

pub struct Generator<'a> {
    config: &'a Config,
    words: WordLists,
    template: Option<Vec<Part>>,  // None = default mode
}
```

**Generierungs-Modi**:

#### Modus 1: Default (kein Template)

Erzeugt Namen nach dem Muster (Wahrscheinlichkeiten aus `config.toml`):
```
[The] <prefix> <word> [of [the] <suffix_adj> <suffix>]
```

Ablauf:
1. Optional Präfix: `gen_bool(prefix_probability)`
2. Optional "The" vor Präfix: `gen_bool(prefix_article_probability)`
3. Wort (erforderlich): `pick(words)`
4. Optional Suffix-Block: `gen_bool(suffix_name_probability)`
   - Fillword: "of"
   - Optional "the": `gen_bool(suffix_article_probability)`
   - Optional Adjektiv: `gen_bool(suffix_adjectiv_probability)`
   - Suffix-Nomen: `pick(suffix_names)`
5. Tokens mit Separator joinen

#### Modus 2: Template

Template-String mit Platzhaltern:
```
"The {word} of {suffix_adj} {suffix}"
"Dreaming {prefix} {word}"
"Only {word}"
```

Ablauf:
1. `parse_template()` → `Vec<Part>` (Literals + Slots)
2. Für jeden Part:
   - `Literal` → unverändert anhängen
   - `Slot` → Wort auswählen (`pick_for_slot()`)
3. Whitespace normalisieren (mehrere Leerzeichen → eins, trim)
4. None, wenn all Slots leer sind (und mind. ein Slot existierte)

**Hilfsfunktionen**:
```rust
fn pick<'a>(list: &'a [String], rng: &mut SeededRng) -> Option<&'a str>
    // Wählt zufälliges Element aus Liste (Index via rng.gen_index)

fn join_tokens<'a>(tokens: impl Iterator<Item = &'a str>, sep: &str) -> String
    // Joined Tokens, filtert Leerstränge
```

**Duplikat-Vermeidung**:
```rust
pub fn generate_unique(
    &self,
    rng: &mut SeededRng,
    used: &mut HashSet<String>,
    max_attempts: usize,
) -> Option<String>
```
- Versucht bis zu `max_attempts` Mal, einen neuen Namen zu generieren
- Prüft HashSet auf Duplikate
- Gibt None zurück, wenn nach max_attempts kein neuer Name gefunden

---

### 5. **cli.rs** — Befehlszeileninterface

Parst Kommandos (über `clap`) und orchestriert die Logik.

**Struktur**:
```rust
#[derive(Parser)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(long, global = true)]
    config: String,
}

#[derive(Subcommand)]
enum Commands {
    Gen { seed, count, systems, template, format },
    List(ListCommand),
    Db(DbCommand),
}

enum ListCommand {
    Systems,
    Languages,
    Classes,
    Words { system, language },
}

enum DbCommand {
    Import { path },
    Info,
}
```

**Hauptablauf für `gen`**:
1. `open_context(config_path)` → `(Config, Db)`
2. `load_wordlists(db, systems_filter)` → `WordLists`
3. `Generator::new()` oder `Generator::with_template()`
4. Seed generieren oder verwenden
5. Loop `count` Mal: `generate_unique()` → Name
6. Ausgeben (text oder json)

**Hauptablauf für `list systems|languages|classes`**:
1. `open_context()`
2. `db.list_*()` → Vec<(name, count)>
3. Tabelle formatieren

**Hauptablauf für `list words`**:
1. `open_context()`
2. `db.list_words(system, language)` → Vec<(word, lang, sys, class)>
3. Tabelle formatieren

**Hauptablauf für `db import`**:
1. `open_context()`
2. `csv_import::read_words(path)` → Vec<WordRecord>
3. `db.insert_words(records)`
4. Erfolgs-Nachricht

**Hauptablauf für `db info`**:
1. `open_context()`
2. `db.stats()` → DbStats
3. Statistiken anzeigen

---

## Designprinzipien

### 1. **Alle SQL in einem Modul**

- `db/mod.rs` ist die einzige Stelle mit SQL
- Andere Module rufen `db.*()` Methoden auf
- Vorteil: Schema-Änderungen sind lokal, Testing einfacher

### 2. **Funktionale Komposition**

- Kleine pure Helper: `pick()`, `join_tokens()`, `parse_template()`
- Generator ist immutable (außer rng)
- Keine globalen Zustände

### 3. **Determinismus über Seed**

- Nur `ChaCha8Rng` für Zufall
- Nie `rand::random()` ohne Seed
- Nie SQL `RANDOM()`
- Ein Seed → identische Ausgabe immer

### 4. **Duplikat-Vermeidung mit Limits**

- `generate_unique()` mit `max_attempts` Parameter
- Verhindert Endlosschleifen
- Gibt auf, wenn Pool erschöpft
- Nutzer wird gewarnt, wenn `--count` nicht erfüllbar

### 5. **Flexible Templates**

- Defaultmodus: Wahrscheinlichkeiten steuern Struktur
- Template-Modus: Nutzer definiert Struktur
- Template-Parser validiert Platzhalter
- Unbekannte Platzhalter → Fehler (nicht ignoriert)

---

## Zukünftige Erweiterungen

### Phase 2 — Etymologie und Origin

**Datenbank-Schema-Erweiterung**:
- Spalte `etymology TEXT` — Kurzbeschreibung der Herkunft
- Spalte `origin_lang TEXT` — Ursprungssprache (grc, lat, non, ...)

**Datenfluss**:
- CSV um Spalten erweitern
- Import-Pipeline: CSV-Spalten → DB
- Generator ignoriert diese noch (können später für Erklärungen verwendet werden)

### Phase 3 — Reverse-Lookup

**Neuer Command**: `find "<beschreibung>" [--count N] [--explain] [--systems <systems>]`

**Datenfluss**:
- Nutzer-Input: "Eine CLI, die Logs synchronisiert"
- Tokenisierung + Stopwörter-Entfernung (DE/EN)
- Keyword-Scoring gegen `word`, `tags`, `system`, `etymology`
- Ranking nach Relevanz
- Ausgabe: ein Wort (oder `--count N`)
- Optional `--explain`: Etymologie + Gründe anzeigen

**Kein Embedding-Overhead** in Phase 3. Nur Textverarbeitung.

### Phase 4 — Semantische Suche (optional)

**Später**: Lokale Embeddings (z. B. ONNX) hinter derselben `find`-Oberfläche.
- Vorteil: bessere semantische Matches
- Nachteil: größeres Binary, höherer Speicher
- Entscheidung: je nach Real-World-Usage

---

## Testing

Aktuell:
- Integrationstests in `tests/` (z. B. deterministische Seed-Tests)
- CLI wird direkt getestet (keine Unit-Test-fokussierte Struktur)

Zukünftig:
- Template-Parser Unit-Tests
- RNG determinism-Tests
- Schema-Migrations-Tests

