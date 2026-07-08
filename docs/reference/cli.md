# Kommandoreferenz — Name Generator

Vollständige Referenz aller CLI-Kommandos und Optionen.

## Übersicht

```
name-generator [GLOBAL-OPTIONS] <COMMAND>
```

Die CLI folgt einer hierarchischen Subcommand-Struktur:
- **gen** — Namen generieren
- **list** — Datenbank durchsuchen
  - **systems** — Verfügbare Systeme auflisten
  - **languages** — Verfügbare Sprachen auflisten
  - **classes** — Wortklassen auflisten
  - **words** — Wörter mit optionalen Filtern auflisten
- **db** — Datenbankoperationen
  - **import** — CSV-Datei in Datenbank importieren
  - **info** — Datenbankstatistiken anzeigen
- **help** — Hilfe für ein Kommando anzeigen

### Globale Optionen

```
--config <CONFIG>
```
Pfad zur Konfigurationsdatei. Standardwert: `config.toml`. Die Datei enthält Wahrscheinlichkeitskonfigurationen und den Datenbankpfad.

```
-h, --help
```
Hilfe anzeigen.

```
-V, --version
```
Versionsinformation anzeigen.

---

## gen — Namen generieren

Generiert ein oder mehrere zufällige Namen aus der Wortdatenbank.

### Syntax

```
name-generator gen [OPTIONS]
```

### Optionen

| Option | Wert | Beschreibung |
|--------|------|-------------|
| `--seed <SEED>` | u64 | Seed für reproduzierbare Generierung. Ohne Angabe wird automatisch ein Seed erzeugt und gedruckt. |
| `--count <COUNT>` | usize | Anzahl der zu generierenden Namen. Standardwert: 1 |
| `--systems <SYSTEMS>` | String | Komma-getrennte Liste von Systemen zum Filtern (z. B. `nature,myth_greek`). Wenn leer, werden alle Systeme verwendet. |
| `--template <TEMPLATE>` | String | Benutzerdefinierte Template-String mit Platzhaltern (siehe unten). Überschreibt die Standardvorlage. |
| `--format <FORMAT>` | text \| json | Ausgabeformat. Standardwert: `text` |
| `--config <CONFIG>` | Path | Konfigurationsdatei-Pfad. Standardwert: `config.toml` |

### Template-Platzhalter

Die folgenden Platzhalter können in `--template` verwendet werden:

| Platzhalter | Wortklasse | Beschreibung |
|------------|-----------|-------------|
| `{prefix}` | prefix | Präfix, wird durch `prefix_probability` kontrolliert |
| `{word}` | noun, proper | Hauptwort (erforderlich im Defaultmodus, optional im Template) |
| `{suffix_adj}` | adj | Adjektiv im Suffix, wird durch `suffix_adjectiv_probability` kontrolliert |
| `{suffix}` | suffix | Suffix-Nomen, wird durch `suffix_name_probability` kontrolliert |

### Beispiele

#### Beispiel 1: Eine Name mit automatischem Seed

```bash
name-generator gen
```

Ausgabe:
```
seed=5070469808648446065
The Crimson sol
```

Wenn kein Seed angegeben wird, erzeugt die CLI automatisch einen Seed und gibt ihn aus (Präfix `seed=`). Dies ermöglicht die Reproduzierbarkeit für später.

#### Beispiel 2: Drei Namen mit Seed 42 (reproduzierbar)

```bash
name-generator gen --seed 42 --count 3
```

Ausgabe:
```
Wandering atlas of Essenz
Heilig silvan
The Iron apex of the verborgen Dawn
```

Derselbe Seed erzeugt immer dieselbe Sequenz in derselben Reihenfolge.

#### Beispiel 3: Namen aus einem bestimmten System

```bash
name-generator gen --systems nature --count 2
```

Ausgabe:
```
seed=5070469808648446065
The Crimson sol
Crimson ember of Doom
```

Das Filter `--systems` akzeptiert komma-getrennte System-IDs. Nur Wörter mit diesen System-Tags werden verwendet.

#### Beispiel 4: Benutzerdefinierte Template

```bash
name-generator gen --seed 42 --template "The {word} of {suffix_adj} {suffix}"
```

Ausgabe:
```
The crater of luminous Glory
```

Template-Platzhalter werden durch zufällig ausgewählte Wörter ersetzt. Literale Texte bleiben erhalten.

#### Beispiel 5: JSON-Ausgabe

```bash
name-generator gen --format json --seed 42 --count 1
```

Ausgabe:
```json
{
  "seed": 42,
  "names": [
    "Wandering atlas of Essenz"
  ]
}
```

JSON-Format enthält den Seed und ein Array von generierten Namen.

---

## list — Datenbank-Übersicht

Erkundet die Wortdatenbank und listet verfügbare Systeme, Sprachen, Wortklassen und einzelne Wörter auf.

### list systems — Alle Systeme anzeigen

Listet alle Systeme und die Anzahl der Wörter pro System.

#### Syntax

```
name-generator list systems
```

#### Beispiel

```bash
name-generator list systems
```

Ausgabe:
```
nature               34
myth_greek           22
craft                21
```

### list languages — Alle Sprachen anzeigen

Listet alle Sprachen und die Anzahl der Wörter pro Sprache.

#### Syntax

```
name-generator list languages
```

#### Beispiel

```bash
name-generator list languages
```

Ausgabe:
```
en                   36
la                   23
de                   18
```

### list classes — Alle Wortklassen anzeigen

Listet alle Wortklassen und die Anzahl der Wörter pro Klasse.

#### Syntax

```
name-generator list classes
```

#### Beispiel

```bash
name-generator list classes
```

Ausgabe:
```
noun                 33
proper               15
prefix               11
suffix               10
adj                  8
```

### list words — Wörter auflisten

Listet alle Wörter, optional gefiltert nach System und/oder Sprache.

#### Syntax

```
name-generator list words [OPTIONS]
```

#### Optionen

| Option | Wert | Beschreibung |
|--------|------|-------------|
| `--system <SYSTEM>` | String | Filtert auf ein bestimmtes System |
| `--language <LANGUAGE>` | String | Filtert auf eine bestimmte Sprache (z. B. `en`, `de`, `la`) |

#### Beispiel

```bash
name-generator list words --system nature --language en
```

Ausgabe (gekürzt):
```
oak                  en / nature / noun
luminous             en / nature / adj
flame                en / nature / noun
ruin                 en / nature / noun
mist                 en / nature / noun
```

Spalten: Wort | Sprache | System | Wortklasse

---

## db — Datenbankoperationen

Verwaltet die lokale Wortdatenbank.

### db import — CSV-Datei importieren

Importiert eine CSV-Datei in die SQLite-Datenbank. Die Datei muss das Format erfüllen (siehe `docs/reference/data-model.md`).

#### Syntax

```
name-generator db import <PATH>
```

#### Argumente

| Argument | Beschreibung |
|----------|-------------|
| `<PATH>` | Pfad zur CSV-Datei |

#### Beispiel

```bash
name-generator db import data/words.csv
```

Ausgabe:
```
Imported 77 words.
```

Die Datenbank wird erstellt oder aktualisiert. Duplikate (nach `id = language_word`) werden durch die neue Version ersetzt.

### db info — Datenbankstatistiken

Zeigt grundlegende Statistiken über die importierten Daten.

#### Syntax

```
name-generator db info
```

#### Beispiel

```bash
name-generator db info
```

Ausgabe:
```
Total words: 77

By language:
  en: 36
  la: 23
  de: 18

By system:
  nature: 34
  myth_greek: 22
  craft: 21
```

---

## Seed-Semantik

### Automatischer Seed

Wenn `--seed` nicht angegeben wird:
1. Die CLI erzeugt einen zufälligen Seed (64-Bit-Wert)
2. Der Seed wird auf stderr/stdout gedruckt (Format: `seed=<n>`)
3. Alle Namen werden mit diesem Seed generiert

**Grund**: Du kannst einen interessanten Lauf später reproduzieren, wenn du den Seed notierst.

### Expliziter Seed

Wenn `--seed N` angegeben wird:
1. Der Seed wird verwendet, nicht generiert
2. Kein `seed=`-Präfix wird gedruckt (nur die Namen)
3. Derselbe Seed erzeugt immer die gleiche Sequenz in der gleichen Reihenfolge

### Beispiel-Workflow

```bash
# Lauf 1: Namen generieren (mit zufälligem Seed)
name-generator gen --count 3
# Ausgabe:
# seed=12345678
# Name A
# Name B
# Name C

# Lauf 2: Seed notieren und später reproduzieren
name-generator gen --seed 12345678 --count 3
# Ausgabe (identisch zu Lauf 1):
# Name A
# Name B
# Name C
```

---

## Konfiguration

Die Datei `config.toml` steuert die Generierungswahrscheinlichkeiten:

```toml
[generator]
prefix_article_probability = 0.2      # "The" vor Präfix
prefix_probability = 0.8              # Präfix überhaupt
suffix_article_probability = 0.3      # "the" im Suffix
suffix_adjectiv_probability = 0.5     # Adjektiv im Suffix
suffix_name_probability = 0.5         # Suffix überhaupt
separator = " "                       # Trennzeichen zwischen Tokens
fillword = "of"                       # Wort zwischen Hauptwort und Suffix

[db]
path = "data/words.db"                # Datenbankpfad
```

Mit `--config <DATEI>` kann eine alternative Konfiguration verwendet werden.

---

## Fehlerbehandlung

| Fehler | Ursache | Lösung |
|--------|--------|--------|
| `no words available - import data first` | Datenbank ist leer | `name-generator db import data/words.csv` ausführen |
| `Failed to read config file` | `config.toml` nicht gefunden | Datei erstellen oder `--config` angeben |
| `Unknown placeholder: {foo}` | Ungültiger Platzhalter im Template | Nur `{prefix}`, `{word}`, `{suffix_adj}`, `{suffix}` verwenden |
| `only N unique names were possible` | Zu wenig Wörter für --count | `--count` reduzieren oder mehr Wörter importieren |

