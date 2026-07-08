# Datenmodell — spoor

Beschreibung des CSV-Formats, des SQLite-Schemas und der Duplikat-Vermeidung.

## CSV-Format

Eingabedatei für `spoor db import <CSV>`. Beispiel: `data/words.csv`

### Spalten

Die CSV muss **genau diese Spalten in dieser Reihenfolge** haben:

| Index | Spalte | Datentyp | Erforderlich | Beschreibung |
|-------|--------|----------|------------|-------------|
| 0 | `word` | String | Ja | Das Wort selbst (z. B. "forge", "luna") |
| 1 | `language` | String | Ja | Sprachcode (z. B. "en", "de", "la") |
| 2 | `word_class` | String | Ja | Wortklasse (s. unten) |
| 3 | `system` | String | Ja | System-ID (z. B. "nature", "myth_greek") |
| 4 | `tags` | String | Nein | Komma-getrennte Tags (z. B. "fire,sky,warmth") |
| 5 | `seed_weight` | Float | Nein | Gewicht für Sampling (Standard: 1.0, zukünftig) |
| 6 | `source` | String | Nein | Herkunft (z. B. "wiktionary", "curated") |
| 7 | `etymology` | String | Nein | Etymologische Erklärung auf Deutsch (z. B. "lat. silva 'Wald'") |
| 8 | `origin_lang` | String | Nein | ISO-Code der Herkunftssprache (z. B. "la", "grc", "ang") |

### Beispiel

```csv
word,language,word_class,system,tags,seed_weight,source,etymology,origin_lang
silvan,la,noun,nature,"forest,tree",1.0,wiktionary,"lat. silva 'Wald'",la
oak,en,noun,nature,"tree,strength",1.0,wiktionary,"altengl. āc 'Eiche', germ. *aikaz",ang
zeus,la,proper,myth_greek,"sky,thunder,king",1.2,curated,"griech. Zeus, idg. *dyeus 'Himmel, Tag'",grc
```

### Wortklassen

| Wertclass | Rolle im Generator | Erklärung |
|-----------|-------------------|-----------|
| `prefix` | `{prefix}` | Präfix im Defaultmodus; optional mit "The" |
| `noun` | `{word}` | Hauptwort; erforderlich im Defaultmodus |
| `proper` | `{word}` | Eigenname; alternativ zu `noun` für Hauptwort |
| `adj` | `{suffix_adj}` | Adjektiv im Suffix; optional mit `suffix_adjectiv_probability` |
| `suffix` | `{suffix}` | Suffix-Nomen; optional mit `suffix_name_probability` |

**Verarbeitung in `load_wordlists()`** (cli.rs):
```rust
match word_class {
    "prefix" => prefixes.push(word),
    "noun" | "proper" => words.push(word),
    "adj" => suffix_adjs.push(word),
    "suffix_noun" | "suffix" => suffix_names.push(word),
    _ => {}  // Ignoriert
}
```

**Hinweis**: `suffix_noun` und `suffix` werden beide als Suffix behandelt.

### Felder-Details

#### `tags` (komma-getrennt, optional)

Tags sind flexible Marker für semantische Suche (Phase 3+). Beispiele:
- `"fire,warmth,energy"` für "flame"
- `"wisdom,strategy,war"` für "athena"
- `"silver,metal,craftsmanship"` für "anvil"

Aktuell ignoriert durch den Generator. Zukünftig für `find` verwendet.

#### `seed_weight` (Float, optional)

Gewicht für gewichtetes Sampling. Standard: 1.0

Aktuell ignoriert. Zukünftig: Höhere Gewichte → Wort wird häufiger ausgewählt.

Beispiel: `zeus,la,proper,myth_greek,...,1.2,...` → 20% häufiger als Standard.

#### `source` (String, optional)

Herkunft des Eintrags. Hilf beim Tracking und Debugging.

Beispiele:
- `"wiktionary"` — aus Wiktionary-Dump
- `"curated"` — manuell ausgewählt
- `"github:kkrypt0nn/wordlists"` — externe Liste

Zukünftig: Für Quellenangabe in Erklärungen (Phase 3+).

---

## sources.yaml — Konfiguration für `db fetch`

Eingabedatei für `spoor db fetch [--file <PATH>]`. Standardpfad: `sources.yaml` im Projektwurzelverzeichnis. Beschreibt Online-Wortquellen, die per HTTP gestreamt und geparst werden (siehe `docs/reference/architecture.md`, Abschnitt "fetch-Modul").

### Format

```yaml
sources:
  - id: kaikki-de
    backend: wiktextract-jsonl
    url: https://kaikki.org/dictionary/German/kaikki.org-dictionary-German.jsonl
    language: de
    system: wiktionary_de
    max_words: 500
```

### Felder

| Feld | Typ | Erforderlich | Beschreibung |
|------|-----|------------|-------------|
| `id` | String | Ja | Eindeutige Quellen-ID. Wird für `--only <ids>` verwendet und in der Progress-Anzeige gezeigt. |
| `backend` | String | Ja | Parser-Typ. Aktuell unterstützt: `wiktextract-jsonl` (s. u.). Unbekannte Werte lassen `load_sources()` mit einer Fehlermeldung abbrechen. |
| `url` | String | Ja | HTTP(S)-URL der Quelldatei. Endet die URL auf `.gz`, wird die Antwort transparent entpackt (gzip). |
| `language` | String | Ja | Sprachcode, der jedem importierten Wort zugewiesen wird (z. B. `de`, `en`, `la`). |
| `system` | String | Ja | System-ID, der jedem importierten Wort zugewiesen wird (z. B. `wiktionary_de`). |
| `max_words` | usize | Nein (Standard: 500) | Maximale Anzahl akzeptierter Wörter. Das Streaming stoppt SOFORT, sobald dieses Limit erreicht ist — der Rest der (oft GB-großen) Datei wird nie gelesen oder heruntergeladen. Kann pro Lauf mit `db fetch --limit N` überschrieben werden (für alle ausgewählten Quellen). |

### Unterstützte Backends

| Backend | Format | Beschreibung |
|---------|--------|-------------|
| `wiktextract-jsonl` | JSON Lines (eine JSON-Zeile pro Wort) | kaikki.org-Exporte des `wiktextract`-Tools. Jede Zeile wird von `parse_wiktextract_line()` in einen `WordRecord` übersetzt: `word` → Wort, `pos` (`noun`/`adj`/`name`) → `word_class` (`noun`/`adj`/`proper`), erste 2 Glosse → `tags`, `etymology_text` → `etymology` (gekürzt, kleingeschrieben). Andere `pos`-Werte (z. B. `verb`) werden übersprungen, nicht importiert. |

Nur diese Backend-Typen haben eine Implementierung im Code — andere Werte in `backend` schlagen beim Laden der Datei fehl (mit einer Liste der unterstützten Typen in der Fehlermeldung).

### Duplikate

Wie bei `db import` gilt: `id` in der Datenbank ist `language_word`. Kommt derselbe Wortstamm mehrfach in der Quelldatei vor (z. B. mehrere Wiktionary-Einträge zum selben Lemma mit unterschiedlichen Bedeutungen), überschreibt der letzte gelesene Eintrag die vorherigen (`INSERT OR REPLACE`). Das ist der Grund, warum die Anzahl der `list systems`-Zeilen nach einem Fetch kleiner sein kann als die Anzahl der als "importiert" gemeldeten Wörter.

---

## SQLite-Schema

Erzeugt durch `db::Db::ensure_schema()`. Die `words.db` wird erneut erstellt oder aktualisiert bei jedem Import.

### Tabelle: `words`

```sql
CREATE TABLE IF NOT EXISTS words (
    id TEXT PRIMARY KEY,
    word TEXT NOT NULL,
    word_class TEXT,
    language TEXT,
    system TEXT,
    tags TEXT,
    seed_weight REAL DEFAULT 1.0,
    source TEXT,
    etymology TEXT,
    origin_lang TEXT
);
```

### Spalten

| Spalte | Typ | PRIMARY KEY | Beschreibung |
|--------|-----|-------------|-------------|
| `id` | TEXT | Ja | Duplikat-Schlüssel: `language_word` (z. B. `en_forge`) |
| `word` | TEXT | Nein | Wort (z. B. `forge`) |
| `word_class` | TEXT | Nein | Wortklasse (prefix, noun, proper, adj, suffix) |
| `language` | TEXT | Nein | Sprachcode (en, de, la, ...) |
| `system` | TEXT | Nein | System-ID (nature, myth_greek, craft, ...) |
| `tags` | TEXT | Nein | Komma-getrennte Tags (optional) |
| `seed_weight` | REAL | Nein | Gewicht (Standard: 1.0) |
| `source` | TEXT | Nein | Quelle (optional) |
| `etymology` | TEXT | Nein | Etymologische Erklärung auf Deutsch (optional) |
| `origin_lang` | TEXT | Nein | ISO-Code der Herkunftssprache (optional) |

### Duplikat-Vermeidung

**Schlüsselelement**: Spalte `id`.

Beim CSV-Import wird `id` aus Sprache und Wort zusammengesetzt:
```rust
id = format!("{}_{}", language, word)
```

Beispiel:
- CSV-Eintrag: `forge,en,noun,craft,...`
- Generierte ID: `en_forge`
- CSV-Eintrag: `forge,de,noun,craft,...` (anderes Wort, andere Sprache)
- Generierte ID: `de_forge`

Diese zwei sind **unterschiedlich** und werden beide eingefügt.

Wenn aber die gleiche Kombination zweimal in der CSV vorkommt (oder in verschiedenen Imports):
- CSV-Lauf 1: `forge,en,noun,craft,fire,1.0,wiktionary`
- CSV-Lauf 2: `forge,en,noun,craft,fire metal,1.1,curated`
- INSERT OR REPLACE: Der zweite Eintrag **überschreibt** den ersten (gleiche `id`)

**Vorteil**:
- Keine Duplikate trotz mehrfacher Imports
- Einfache Weise, Einträge zu aktualisieren
- Keine zusätzliche Deduplizierungs-Logik nötig

---

## Datenfluss: Import

```
data/words.csv
    ↓ (csv::Reader)
Vec<csv::StringRecord>
    ↓ (WordRecord::parse_csv_record)
Vec<WordRecord> {
    id: "language_word",
    word: "...",
    word_class: "...",
    ...
}
    ↓ (db.insert_words)
SQL: INSERT OR REPLACE INTO words (id, word, ...)
    ↓
words.db (SQLite)
```

### Fehlerverwaltung

- **CSV-Parsing-Fehler**: Angebrochen bei erstem Fehler (csv::Error)
- **SQL-Fehler**: Angebrochen bei erstem Fehler (Transaction rollback)
- **Fehlende Spalte**: Standardwert oder None (je nach CSV)

---

## Datenfluss: Generierung (Query)

```
words.db
    ↓ (db.words_by_class with system filter)
SQL: SELECT word, word_class FROM words WHERE system IN (...)
    ↓
Vec<(String, String)> // (word, class)
    ↓ (load_wordlists in cli.rs)
Gruppiert nach word_class:
    WordLists {
        prefixes: ["The", "Wandering", ...],
        words: ["forge", "luna", ...],
        suffix_adjs: ["luminous", "Heilig", ...],
        suffix_names: ["atlas", "silvan", ...],
    }
    ↓ (Generator::generate_one)
Wählt Wörter via ChaCha8Rng:
    "The" + "Wandering" + "forge" + "of" + "luminous" + "atlas"
    ↓
Name: "The Wandering forge of luminous atlas"
```

---

## Implementierte Schema-Erweiterungen

### Phase 2 — Etymologie und Herkunftssprache ✅ FERTIG

**Neue Spalten** (in CSV und Schema):

```sql
ALTER TABLE words ADD COLUMN etymology TEXT;
ALTER TABLE words ADD COLUMN origin_lang TEXT;
```

CSV-Spalte 8 (nach `source`): `etymology`
CSV-Spalte 9: `origin_lang`

Beispiel:
```
zeus,la,proper,myth_greek,"sky,thunder",1.2,curated,"griech. Zeus, idg. *dyeus 'Himmel, Tag'",grc
```

**Nutzen**:
- Phase 3 `find`: Begründung ausgeben ("athena" kommt aus griechisch "athenai" = Weisheit)
- Nullable (zukünftige Datenbestände können leer sein)

**Backward-Kompatibilität**: 7-spaltige CSVs (ohne `etymology` und `origin_lang`) werden beim Import mit leeren Werten importiert.

## Zukünftige Schema-Erweiterungen

### Phase 3 — Reverse-Lookup mit FTS

**Optional**: SQLite Full-Text-Search (FTS5)

```sql
CREATE VIRTUAL TABLE words_fts USING fts5(word, tags, etymology);
```

**Nutzen**:
- Schnelle Textsuche über `word`, `tags`, `etymology`
- Keyword-Matching für `find "<beschreibung>"`

**Implementierung**: Lazily (nur wenn `find` Command verwendet).

---

## Aktuelle Datenbasis

**Quelle**: `data/words.csv`

**Größe**: 77 Wörter (Stand Phase 0)

**Zusammensetzung**:

| System | Sprache | Count | Wortklassen |
|--------|---------|-------|-----------|
| nature | en | 8 | noun, adj, proper |
| nature | la | 7 | noun, proper |
| nature | de | 4 | noun |
| myth_greek | la | 6 | proper |
| craft | en | 7 | noun, adj |
| craft | la | 2 | noun |
| — | — | 36 | (insgesamt) |

**Hinweis**: `data/words.db` ist **generiert** durch `db import` und wird **nicht versioniert** (in `.gitignore`).

---

## Best Practices für Dateneinträge

### 1. **IDs sind eindeutig pro Sprache**

Nicht: `id="forest"`
Sondern: `id="en_forest"`, `id="de_wald"`, `id="la_silvan"`

Die CLI/Datenbankcode handhaben dies automatisch.

### 2. **Tags sollten semantisch relevant sein**

Nicht: `tags="thing,object"`
Sondern: `tags="fire,warmth,energy"` für "flame"

Tags werden für Phase 3 (`find`) verwendet.

### 3. **source sollte nachverfolgbar sein**

Beispiele:
- `source="wiktionary"` — aus Wiktionary-Dump (Phase 2+)
- `source="curated"` — manuell ausgewählt
- `source="github:kkrypt0nn/wordlists"` — von externer Liste
- `source="project:internal"` — interne Sammlung

### 4. **seed_weight bleibt 1.0 (vorerst)**

Wird in Phase 2+ unterstützt. Nutzer alle auf Standard (1.0) setzen.

### 5. **Leerzeichen trimmen**

Parser machen das automatisch, aber in CSV besser sauberer.

```csv
# Gut:
forge,en,noun,craft,...

# Nicht gut:
forge  ,  en  ,  noun  ,...
```

