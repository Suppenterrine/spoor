# Architektur — spoor

Übersicht der modularen Architektur und der Datenflüsse.

## Zero-Setup: Eingebettete Basisdaten und Auto-Bootstrap

Das Binary `spoor` ist sofort nach dem Download einsatzfähig — ohne Repository, ohne `config.toml`, ohne manuelle Datenbankinitialisierung.

### Embedded Seed Data

- `const SEED_WORDS_CSV: &str = include_str!("../data/words.csv")` in `src/lib.rs`
- 77 kuratierte Grundwörter (natur, myth_greek, craft)
- Enthalten bereits Etymologie, Gewichte und Mehrsprachigkeit
- Werden zur Compile-Zeit eingebettet

### Auto-Bootstrap auf Kaltstart

**Bootstrap-Logik in `cli.rs` — Funktion `open_context_bootstrapped()`**:

1. Config laden:
   - Wenn `--config` nicht angegeben: Standard `config.toml` mit `explicit=false`
   - Fehlende Config-Datei → `Config::default()` (keine Fehlermeldung)
   - Wenn `--config` angegeben: Datei MUSS existieren, sonst Fehler

2. DB-Pfad auflösen:
   - Wenn `config.toml` da: Wert aus `[db].path` verwenden
   - Wenn `config.toml` fehlt oder `path` leer: `default_db_path()`
   - `default_db_path()`: `$APPDATA/spoor/words.db` (Windows) oder `~/.local/share/spoor/words.db` (Linux) oder Fallback `data/words.db`

3. Elternverzeichnis erstellen:
   - `std::fs::create_dir_all(db_path.parent())` — kein Fehler, wenn schon da

4. DB öffnen/erstellen:
   - `Db::open(db_path)` — erstellt die Datei automatisch, wenn nicht vorhanden

5. **Bootstrap-Entscheidung**:
   - Prüfe: Existiert die Datei **vor** `Db::open()`?
   - **Nein** → Datenbankdatei ist neu:
     - `db.import_csv_reader(SEED_WORDS_CSV.as_bytes())` — importiert 77 Wörter in einer Transaktion
     - Gibt `ImportReport { imported, unknown_class }` zurück
     - Schreibe zu **stderr** (nicht stdout): `"Initialized word database with N curated words at <path>"`
     - Rückgabe: `(Config, Db, true)` — war_bootstrapped = true
   - **Ja** → Datenbankdatei existiert bereits:
     - Keine Aktion
     - Rückgabe: `(Config, Db, false)` — war_bootstrapped = false

**Verhalten**:
- Erster Aufruf (z. B. `spoor find "..."` in frischem Verzeichnis):
  - Stderr: `Initialized word database with 77 curated words at C:\Users\...\AppData\Roaming\spoor\words.db`
  - Stdout: Suchergebnis (z. B. "zeus")
- Zweiter Aufruf (derselbe oder anderer Befehl):
  - **Keine** Init-Meldung (DB existiert bereits)
  - Stdout: Ergebnis

**Im Repository**:
- `config.toml` vorhanden mit `[db] path = "data/words.db"`
- Beim Start wird `data/words.db` verwendet (wurde bereits von Fetch importiert)
- **Kein Bootstrap** (Datei existiert)

### Prioritätsreihenfolge für Datenbank-Pfad

1. Explizite `--config <PATH>` → `<PATH>` parsen, `db.path` verwenden
2. Standard `config.toml` vorhanden → dessen `db.path` verwenden
3. Standard `config.toml` **fehlt** → `default_db_path()` (Nutzerdatenverzeichnis)
4. `db.path` in Config leer oder fehlt → `default_db_path()` (Fallback in serde `#[serde(default)]`)
5. `default_db_path()` kann auch `None` sein (z. B. auf exotischen Systemen) → Fallback `data/words.db`

### Code in config.rs

```rust
pub fn default_db_path() -> PathBuf {
    dirs::data_dir()
        .map(|d| d.join("spoor").join("words.db"))
        .unwrap_or_else(|| PathBuf::from("data/words.db"))
}

impl Config {
    pub fn load(path: &str, explicit: bool) -> anyhow::Result<Self> {
        match fs::read_to_string(path) {
            Ok(content) => { /* parse und return */ }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if explicit { Err(...) } else { Ok(Config::default()) }
            }
            Err(e) => Err(e),
        }
    }
}
```

## Modulstruktur

```
src/
├── main.rs              # Einstiegspunkt, delegiert an CLI
├── lib.rs               # Re-exports öffentlicher Schnittstellen
├── cli.rs               # Kommandozeileninterface (clap)
├── config.rs            # Konfiguration (TOML-Parsing)
├── db/mod.rs            # Datenbankoperationen (SQLite)
├── sources.rs           # sources.yaml laden + validieren (SourceSpec, Backend)
├── fetch/mod.rs         # Streaming-Download-Engine + wiktextract-JSONL-Parser
├── generator/
│   ├── mod.rs           # Öffentliche Exports
│   ├── rng.rs           # SeededRng (ChaCha8-Wrapper)
│   └── template.rs      # Template-Parser und Generator
├── lookup/mod.rs        # Reverse-Lookup (Konzeptbrücke + IDF-Scoring)
├── translit.rs          # Lateinische Transliteration (Fallback) + Diakritika-Folding
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
db.import_csv() [Streaming]:
  - Öffnet CSV-Reader mit Iterator (keine Vec-Pufferung)
  - Startet SQLite-Transaktion
  - Prepared Statement wird einmal erstellt
  - Für jeden CSV-Record:
    • WordRecord::parse_csv_record()
    • Zählt unbekannte word_class Werte
    • Führt INSERT OR REPLACE aus
  - Transaktion committen
    ↓
ImportReport { imported: usize, unknown_class: usize }
    ↓
words.db (aktualisiert)

### Lookup-Pipeline (zweistufig, semantisch)

```
Query: "Baum", optional Systems: ["wiktionary_la"]
    ↓
lookup::tokenize(query) → ["baum"]   (Stoppwörter-Filter DE/EN)
    ↓
Stufe A — build_concepts (Konzeptbrücke):
  - jeder Token ist ein Konzept (Gewicht 1.0)
  - Token nennt ein Wort im Brücken-Bestand (ganze DB, auch bei --systems)?
    → dessen englische Glossen-Tokens werden Konzepte (0.7, max. 8/Token)
      "baum" → de/Baum (Glosse "tree") → Konzept "tree"
  - Datamuse-Kandidaten (--online) → Konzepte (0.6)
    ↓
db.all_records(systems) [WHERE system IN, SQL] → Vec<WordRecord>
    ↓
Stufe B — rank_semantic:
  - IDF über alle Glossen-Tokens der Kandidaten (gedeckelt)
  - pro Record: Konzept × stärkstes Feld (Glosse mit IDF ODER Wort),
    plus Wort-Präfix/System/Etymologie für direkte Tokens
  - Anti-Echo: word == Query-Token → Record raus (außer --allow-echo)
  - score × seed_weight × Herkunfts-Faktor (la/grc/he 1.3, el 1.15)
    ↓
Filter: score > 0
    ↓
Sort: score DESC → seed_weight DESC → word ASC (deterministisch)
    ↓
lookup::explain(match) → "word — etymology (lang) · System: sys · Treffer: baum → tree (bruecke)"
    ↓
Ausgabe (text oder json)
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
    pub word_class: Option<String>,    // prefix, noun, proper, adj, suffix, suffix_noun
    pub language: Option<String>,      // en, de, la, ...
    pub system: Option<String>,        // nature, myth_greek, craft, ...
    pub tags: Option<String>,          // Tags (z. B. fire,sky)
    pub seed_weight: f64,              // Gewicht für Ranking
    pub source: Option<String>,        // wiktionary, curated, ...
    pub etymology: Option<String>,     // Herkunftsbeschreibung
    pub origin_lang: Option<String>,   // Ursprungssprache (grc, lat, non, ...)
}

pub struct Db {
    conn: Connection,
}

pub struct DbStats {
    pub total: usize,
    pub by_language: Vec<(String, usize)>,
    pub by_system: Vec<(String, usize)>,
}

pub struct ImportReport {
    pub imported: usize,               // Anzahl importierter Zeilen
    pub unknown_class: usize,          // Anzahl Zeilen mit unbekanntem word_class
}
```

**Verantwortung**:
- SQLite-Verbindung verwalten
- Schema erstellen (`ensure_schema`)
- Wörter einfügen/ersetzen (`insert_words`, `import_csv`)
- `import_csv(path)`: Stream-Import aus CSV, zählt unbekannte word_class, gibt ImportReport zurück
- `all_records(systems: Option<&[String]>)`: Filtert in SQL (WHERE system IN) nicht in Rust
- `words_by_class(systems)`: Filtered list by systems for word loading
- Abfragen: `list_systems()`, `list_languages()`, `list_classes()`, `list_words()`, `stats()`
- Alle SQL-Transaktionen (Atomarität)
- Hilfsfunktion `in_clause(n)` für dynamische WHERE IN Placeholders

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

### 5. **lookup/mod.rs** — Reverse-Lookup (Konzeptbrücke + IDF-Scoring)

Implementiert die zweistufige semantische Suche nach Wörtern zu einer
Nutzfallbeschreibung. Kern-Idee: Alle kaikki-Quellen stammen aus dem
englischen Wiktionary — ihre **englischen Glossen sind eine deterministische
Interlingua**, über die Query-Sprache und Zielsprachen verbunden werden.

**Datentypen**:
```rust
pub struct Match {
    pub record: WordRecord,
    pub score: f64,
    pub matched: Vec<String>,  // Pfade, z. B. ["baum → tree (bruecke)"]
}

struct Concept { term, weight, via: Option<String>, online: bool }
    // via = Brückenwort für den Pfad in der Ausgabe

pub fn tokenize(query: &str) -> Vec<String>
    // Lowercase, split on non-alphanumeric, filter stopwords, dedup

pub fn rank_semantic(records, bridge_records, query, candidates, allow_echo) -> Vec<Match>
    // Vollständiges Ranking; bridge_records = Vokabular für Stufe A
    // (bei --systems die GANZE Datenbank, nicht der Filter)

pub fn rank(records, query) / rank_with_candidates(records, query, candidates)
    // Wrapper: records fungieren selbst als Brücke, allow_echo = false

pub fn explain(m: &Match) -> String
    // German format: "word — etymology (lang) · System: sys · Treffer: ..."
```

**Stufe A — Konzepte (build_concepts)**:
1. Jeder Query-Token → Konzept (Gewicht 1.0)
2. Token nennt ein Wort im Brücken-Bestand → dessen Glossen-Tokens werden
   Konzepte (Gewicht 0.7, max. 8 pro Token, `via` merkt sich den Pfad)
3. Datamuse-Kandidaten → Konzepte (Gewicht 0.6, Label `semantisch`)

Glossen-Tokens durchlaufen `tokenize` plus `GLOSS_NOISE`-Filter
(Wörterbuch-Füllwörter wie "one", "person", "state") und Mindestlänge 3.

**Stufe B — Scoring (score_record_semantic)**:
- **Ein Konzept = ein Beleg**: pro Record zählt nur das stärkste Feld —
  Glossen-Treffer `1.2 × Gewicht × IDF` ODER Wort-Treffer `4.0 × Gewicht`
  (Wort-Treffer nur für Brücken-/Online-Konzepte; das direkte Token-Wort-Paar
  ist das Echo)
- IDF: `1 + ln(N/df)`, geclamped auf [1.0, 6.0] — seltene Glossen-Tokens
  zählen mehr, Einzelstücke dominieren nicht
- Termvergleich mit Plural-Toleranz (trailing-s) und bidirektionalem Präfix (≥5)
- Direkte Tokens zusätzlich: Wort-Präfix 1.5, System 2.0, Etymologie-Substring 1.0

**Anti-Echo**: `word == Query-Token` → Record wird übersprungen. Der North
Star will Assoziation, nicht das Anfragewort zurück. `--allow-echo` stellt
das alte Verhalten her (+5.0 für den Exakttreffer).

**Herkunfts-Bonus**: `score × seed_weight × origin_factor` —
la/grc/he/non/ang/goh ×1.3, el ×1.15, sonst ×1.0.

**Deterministische Sortierung**: Score DESC → seed_weight DESC → word ASC.

**Transliteration im Ranking**: Wort-Vergleiche (Konzept-Wort-Treffer,
Echo-Check) laufen zusätzlich gegen die Diakritika-gefaltete Latin-Form
(`translit`-Spalte), sodass "logos" das grc-Wort λόγος (lógos) trifft.

**Beispiel**:
- Query: "Baum"
- Stufe A: Token "baum"; de/Baum (Glosse "tree") → Konzept "tree" (0.7, via baum)
- Stufe B: la/arboretum (Glosse "...tree...") → 1.2 × 0.7 × idf("tree") × 1.3 (la)
- de/Baum selbst: Echo → gefiltert
- Ausgabe: `arboretum — ... · Treffer: baum → tree (bruecke)`

---

### 6. **cli.rs** — Befehlszeileninterface

Parst Kommandos (über `clap`) und orchestriert die Logik.

**Struktur**:
```rust
#[derive(Parser)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(long, global = true)]
    config: Option<String>,  // None = use default (no error if missing)
}

#[derive(Subcommand)]
enum Commands {
    Gen { seed, count, systems, template, format },
    Find { query, count, systems, explain, format },
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
    Fetch { file, only, limit },
}
```

**Bootstrap-Flow pro Befehl**:
```
Cli::run()
    ↓
Alle Subcommands → `open_context_bootstrapped(self.config.as_deref())`
    ↓
Config::load(path, explicit)
    + default_db_path() if missing
    + create parent dir
    + check DB exists (vor open)
    ↓
Falls DB nicht existiert:
    + import_csv_reader(SEED_WORDS_CSV)
    + eprintln!("Initialized...")
    + return (cfg, db, was_bootstrapped=true)
    ↓
Falls DB existiert:
    + return (cfg, db, was_bootstrapped=false)
    ↓
Subcommand läuft (keine weitere Bootstrap-Logik nötig)
```

**Hauptablauf für `gen`**:
1. `open_context(config_path)` → `(Config, Db)`
2. `load_wordlists(db, systems_filter)` → `WordLists`
3. `Generator::new()` oder `Generator::with_template()`
4. Seed generieren oder verwenden
5. Loop `count` Mal: `generate_unique()` → Name
6. Ausgeben (text oder json)

**Hauptablauf für `find`**:
1. `open_context(config_path)` → `(Config, Db)`
2. Parse systems filter (if provided) → `Vec<String>`
3. `db.all_records(systems)` [WHERE system IN SQL] → Vec<WordRecord>
4. `lookup::rank(records, query)` → Vec<Match>
5. Wenn keine Matches: stderr-Meldung, exit 1
6. Erste `count` Matches ausgeben:
   - Ohne `--explain`: nur `word` pro Zeile
   - Mit `--explain`: `lookup::explain(match)` pro Zeile
7. Mit `--format json`: `FindOutput { query, matches: [...] }`

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
2. `db.import_csv(path)` [Streaming]:
   - CSV-Iterator direkt in Transaktion
   - Prepared Statement wird einmal erzeugt
   - Zählt unbekannte word_class Werte
   - Gibt ImportReport { imported, unknown_class } zurück
3. Print "Imported N words."
4. Falls unknown_class > 0: Print "Warning: N words have an unrecognized word_class and will be ignored by 'gen'."

**Hauptablauf für `db info`**:
1. `open_context()`
2. `db.stats()` → DbStats
3. Statistiken anzeigen

---

### 7. **sources.rs + fetch/mod.rs** — Datenimport per Download (Phase 4a)

Lädt Wortquellen direkt von Online-Wörterbüchern (aktuell: kaikki.org-Wiktionary-Exporte) und importiert sie in die Datenbank, ohne die Quelldatei jemals vollständig herunterzuladen oder zu puffern.

**sources.rs**:
```rust
pub enum Backend { WiktextractJsonl }  // aktuell einziger unterstützter Typ

pub struct SourceSpec {
    pub id: String,
    pub backend: Backend,
    pub url: String,
    pub language: String,
    pub system: String,
    pub max_words: usize,   // Standard: 500
}

pub struct SourcesConfig { pub sources: Vec<SourceSpec> }

pub fn load_sources(path) -> anyhow::Result<SourcesConfig>
```
Liest und validiert `sources.yaml`; unbekannte `backend`-Werte brechen mit einer Fehlermeldung ab, die die unterstützten Typen auflistet.

**fetch/mod.rs — Streaming-Pipeline**:
```rust
pub struct FetchReport { pub id, pub accepted, pub skipped, pub bytes_read, pub error: Option<String> }
pub struct FetchOutcome { pub reports: Vec<FetchReport>, pub total_inserted: usize }

pub trait FetchProgress: Sync {
    fn on_update(&self, id: &str, bytes: u64, accepted: usize, skipped: usize);
    fn on_done(&self, id: &str, report: &FetchReport);
    fn on_error(&self, id: &str, msg: &str);
}

pub fn parse_wiktextract_line(line: &str, spec: &SourceSpec) -> anyhow::Result<Option<WordRecord>>
pub fn consume_jsonl<R: Read>(reader, spec, bytes_read_fn, on_progress, on_batch) -> FetchReport
pub fn fetch_all(db: &mut Db, specs: &[SourceSpec], progress: &dyn FetchProgress) -> anyhow::Result<FetchOutcome>
```

**Datenfluss pro Quelle**:
```
SourceSpec.url
    ↓ ureq::get(url).timeout(60s).call()  [HTTP, kein Zwischenspeichern des Bodys]
Response::into_reader()  [impl Read]
    ↓ CountingReader (AtomicU64, für Fortschrittsanzeige "X MB gelesen")
    ↓ falls url endet auf ".gz": flate2::GzDecoder
BufReader::lines()
    ↓ pro Zeile: parse_wiktextract_line()
    - Ok(Some(rec))  → akzeptiert, in Batch (100 Stück) sammeln
    - Ok(None)       → übersprungen (falscher word_class, Multiword,
                       KEINE brauchbare Glosse: Flexionsformen (form_of/alt_of,
                       Junk-Sense-Tags), Buchhaltungs-Glossen wie "surname",
                       "alternative form of", "plural of" — der Bestand soll
                       Bedeutungsträger enthalten, keine Wörterbuch-Verweise)
    - Err(_)         → übersprungen (kaputte JSON-Zeile bricht den Fetch NICHT ab)
    ↓ sobald accepted == spec.max_words: SOFORT abbrechen (Rest der Datei wird nie gelesen)
    ↓ Batch (voll oder letzter Rest) → mpsc-Channel
Haupt-Thread: db.insert_words(batch) [eine Transaktion pro Batch]
    ↓
FetchReport { accepted, skipped, bytes_read, error: None }
```

**Nebenläufigkeit — Ein-Schreiber-Prinzip**:
- Pro Quelle läuft EIN Worker-Thread (`std::thread::scope`), der ausschließlich liest, parst und über einen `mpsc`-Channel sendet.
- Der Haupt-Thread ist der EINZIGE Thread, der `Db` anfasst: er nimmt Batches aus dem Channel entgegen und fügt sie in eigenen Transaktionen ein (`Db::insert_words`, kein separater SQL-Code — dieselbe `INSERT OR REPLACE`-Logik wie bei `db import`).
- `FetchProgress`-Methoden (`on_update`, `on_error`) werden DIREKT von den Worker-Threads aufgerufen (der Trait verlangt `Sync`), weil sie nur die UI aktualisieren und nie die Datenbank berühren. `on_done` wird vom Haupt-Thread aufgerufen, nachdem der letzte Batch der Quelle eingefügt wurde.
- Schlägt eine Quelle fehl (Netzwerkfehler, ungültige URL, Timeout), wird das über `on_error` gemeldet und im `FetchReport.error` festgehalten — die anderen Quellen laufen unbeeinflusst weiter. `fetch_all` selbst gibt `Ok` zurück, auch wenn einzelne Quellen fehlschlugen.

**Streaming statt Puffern**:
- Es wird nie der komplette HTTP-Body im Speicher gehalten — `BufReader::lines()` liest zeilenweise direkt aus dem (ggf. entpackten) Netzwerk-Stream.
- Frühzeitiger Abbruch (`accepted == max_words`) ist der Kernpunkt: kaikki.org-Dumps sind GB-groß, aber es werden oft nur die ersten paar hundert Zeilen wirklich benötigt.

**cli.rs — `db fetch`**:
- `--file <PATH>` (Standard: `sources.yaml`), `--only <ids>` (Komma-getrennt, nutzt dieselbe Split-Hilfsfunktion wie `--systems`), `--limit <N>` (überschreibt `max_words` für alle ausgewählten Quellen).
- UI: `indicatif::MultiProgress` mit einer Spinner-`ProgressBar` pro Quelle (docker-compose-artig). `CliFetchProgress` implementiert `FetchProgress` und aktualisiert/beendet die jeweilige Zeile (`✔`/`✖`).

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

