# Kommandoreferenz — spoor

Vollständige Referenz aller CLI-Kommandos und Optionen.

> Alle Beispielausgaben in diesem Dokument beruhen auf dem eingebetteten Basisbestand (77 kuratierte Wörter, frische Installation). Nach `spoor db fetch` wächst der Bestand — Trefferlisten und `gen`-Ausgaben fallen dann anders aus (bei gleichem Seed und gleichem Bestand aber stets reproduzierbar).

## Einstiegspunkt: Bare Invocation (keine Argumente)

```bash
$ spoor
```

Zeigt einen Statusbildschirm mit Anleitung (Beispiel: frische Installation mit den 77 eingebetteten Basiswörtern):

```
spoor 0.1.0 — folge der Bedeutung zum Namen

  Wortbestand: 77 Woerter (en 36 · la 23 · de 18)
  Datenbank:   C:/Users/du/AppData/Roaming/spoor/words.db

WOMIT MOECHTEST DU STARTEN?

  Einen Namen zum Anwendungsfall finden:
    spoor find "werkzeug fuer wald und baum" --explain

  Zufaellige Namen generieren (reproduzierbar):
    spoor gen --seed 42 --count 5

  Mehr Woerter laden (kaikki.org, konfiguriert in sources.yaml):
    spoor db fetch --limit 1000

Alle Kommandos: spoor help
```

Der Bildschirm zeigt:
- **Versionsnummer**
- **Wortbestandstatistik** (Gesamtzahl + Sprachen-Verteilung)
- **Drei häufigste Anwendungsfälle** mit Befehlen zum Kopieren
- **Verweis auf `spoor help`** für alle Kommandos

Exit-Code: **0** (Erfolg)

---

## Übersicht

```
spoor [GLOBAL-OPTIONS] <COMMAND>
```

Die CLI folgt einer hierarchischen Subcommand-Struktur:
- **gen** — Namen generieren
- **find** — Ein passendes Wort für eine Nutzfallbeschreibung suchen
- **list** — Datenbank durchsuchen
  - **systems** — Verfügbare Systeme auflisten
  - **languages** — Verfügbare Sprachen auflisten
  - **classes** — Wortklassen auflisten
  - **words** — Wörter mit optionalen Filtern auflisten
- **db** — Datenbankoperationen
  - **import** — CSV-Datei in Datenbank importieren
  - **info** — Datenbankstatistiken anzeigen
  - **fetch** — Wortquellen aus `sources.yaml` per HTTP herunterladen und importieren
- **help** — Hilfe für ein Kommando anzeigen

## Zero-Setup: Direkt nach dem Download funktioniert spoor

Das Binary ist **sofort einsatzfähig**. Es gibt keine Abhängigkeiten oder manuelle Initialisierung:

- **Embedded Seed Data**: 77 kuratierte Grundwörter sind im Binary enthalten
- **Auto-Bootstrap**: Beim ersten Aufruf wird die Datenbank automatisch im Nutzer-Datenverzeichnis angelegt und mit den Basisdaten initialisiert
- **Optional Config**: `config.toml` ist optional. Ohne Datei werden vernünftige Standardwerte verwendet
- **System-Integration**: Datenbankpfad wird im Nutzer-Datenverzeichnis erstellt (z. B. `~/.local/share/spoor/` auf Linux, `%APPDATA%/spoor/` auf Windows)

**Beispiel — Kaltstart**:
```bash
./spoor find "sky thunder king"
```

Ausgabe (erster Aufruf):
```
Initialized word database with 77 curated words at C:\Users\...\AppData\Roaming\spoor\words.db
zeus
```

Ausgabe (zweiter Aufruf — keine Init-Meldung mehr):
```
zeus
```

### Globale Optionen

```
--config <CONFIG>
```
Pfad zur Konfigurationsdatei. Standardwert: `config.toml`.
- **Wenn die Datei existiert**: wird verwendet (Wahrscheinlichkeiten + Datenbankpfad)
- **Wenn die Datei fehlt und nicht explizit angegeben**: Standardwerte werden verwendet (keine Fehlermeldung)
- **Wenn die Datei fehlt und explizit angegeben (`--config /pfad/datei.toml`)**: Fehler

Die Datei enthält Wahrscheinlichkeitskonfigurationen und den Datenbankpfad:
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
path = "data/words.db"  # optional; Standard: Nutzer-Datenverzeichnis
```

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
spoor gen [OPTIONS]
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
spoor gen
```

Ausgabe:
```
seed=5070469808648446065
The Crimson sol
```

Wenn kein Seed angegeben wird, erzeugt die CLI automatisch einen Seed und gibt ihn aus (Präfix `seed=`). Dies ermöglicht die Reproduzierbarkeit für später.

#### Beispiel 2: Drei Namen mit Seed 42 (reproduzierbar)

```bash
spoor gen --seed 42 --count 3
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
spoor gen --systems nature --count 2
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
spoor gen --seed 42 --template "The {word} of {suffix_adj} {suffix}"
```

Ausgabe:
```
The crater of luminous Glory
```

Template-Platzhalter werden durch zufällig ausgewählte Wörter ersetzt. Literale Texte bleiben erhalten.

#### Beispiel 5: JSON-Ausgabe

```bash
spoor gen --format json --seed 42 --count 1
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

## find — Wort für Nutzfallbeschreibung suchen

Findet ein oder mehrere Wörter, die zu einer Beschreibung passen. Nutzt Relevanz-Scoring nach Wort, Tags, System und Etymologie.

### Syntax

```
spoor find <QUERY> [OPTIONS]
```

### Argumente

| Argument | Beschreibung |
|----------|-------------|
| `<QUERY>` | Nutzfallbeschreibung (z. B. "sky thunder king" oder "Werkzeug für Wald") |

### Optionen

| Option | Wert | Beschreibung |
|--------|------|-------------|
| `--count <COUNT>` | usize | Anzahl der gesuchten Wörter. Standardwert: 1 |
| `--systems <SYSTEMS>` | String | Komma-getrennte Liste von Systemen zum Filtern (z. B. `nature,myth_greek`). |
| `--explain` | flag | Zeigt detaillierte Erklärungen (Etymologie, Herkunftssprache, System, Treffer). |
| `--format <FORMAT>` | text \| json | Ausgabeformat. Standardwert: `text` |
| `--config <CONFIG>` | Path | Konfigurationsdatei-Pfad. Standardwert: `config.toml` |

### Scoring-Regeln

Jeder Token der Query wird gegen alle Datensätze gewertet:
- **Wort exakt** (Hauptwort-Match): 5.0 Punkte
- **Wort Substring** (min. 3 Zeichen): 2.0 Punkte
- **Tag exakt**: 3.0 Punkte
- **Tag Substring** (min. 3 Zeichen): 1.5 Punkte
- **System Match**: 2.0 Punkte
- **Etymologie Substring** (min. 3 Zeichen): 1.0 Punkte

Jeder Token wertet jede Feldkategorie höchstens einmal. Die Gesamtpunktzahl wird mit dem `seed_weight` des Worts multipliziert. Sortierung: Score (DESC) → seed_weight (DESC) → Wort (ASC).

### Beispiele

#### Beispiel 1: Ein Wort für englische Götter-Mythologie

```bash
spoor find "sky thunder king"
```

Ausgabe:
```
zeus
```

Das Wort "zeus" matcht die Tags "sky", "thunder" und "king" exakt.

#### Beispiel 2: Deutsche Wörter mit Erklärungen

```bash
spoor find "Werkzeug für Wald und Baum" --count 3 --explain
```

Ausgabe:
```
wald — ahd. wald, germ. *walþuz (goh) · System: nature · Treffer: wald (word), wald (etymology)
silvan — lat. silva 'Wald' (la) · System: nature · Treffer: wald (etymology)
```

Stoppwörter ("für", "und") werden gefiltert; "wald" trifft das Wort selbst und die Etymologien beider Ergebnisse.

#### Beispiel 3: JSON-Ausgabe

```bash
spoor find "light" --format json
```

Ausgabe:
```json
{
  "query": "light",
  "matches": [
    {
      "word": "helios",
      "score": 3.6,
      "etymology": "griech. Helios 'Sonne', idg. *s(w)el-",
      "origin_lang": "grc",
      "system": "myth_greek",
      "tags": "sun,light",
      "matched": ["light (tag)"]
    }
  ]
}
```

---

## list — Datenbank-Übersicht

Erkundet die Wortdatenbank und listet verfügbare Systeme, Sprachen, Wortklassen und einzelne Wörter auf.

### list systems — Alle Systeme anzeigen

Listet alle Systeme und die Anzahl der Wörter pro System.

#### Syntax

```
spoor list systems
```

#### Beispiel

```bash
spoor list systems
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
spoor list languages
```

#### Beispiel

```bash
spoor list languages
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
spoor list classes
```

#### Beispiel

```bash
spoor list classes
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
spoor list words [OPTIONS]
```

#### Optionen

| Option | Wert | Beschreibung |
|--------|------|-------------|
| `--system <SYSTEM>` | String | Filtert auf ein bestimmtes System |
| `--language <LANGUAGE>` | String | Filtert auf eine bestimmte Sprache (z. B. `en`, `de`, `la`) |

#### Beispiel

```bash
spoor list words --system nature --language en
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
spoor db import <PATH>
```

#### Argumente

| Argument | Beschreibung |
|----------|-------------|
| `<PATH>` | Pfad zur CSV-Datei |

#### Beispiel

```bash
spoor db import data/words.csv
```

Ausgabe:
```
Imported 77 words.
```

Die Datenbank wird erstellt oder aktualisiert. Duplikate (nach `id = language_word`) werden durch die neue Version ersetzt.

### db fetch — Wortquellen herunterladen

Lädt Wörter direkt von den in `sources.yaml` konfigurierten Online-Quellen (aktuell: kaikki.org-JSONL-Exporte von Wiktionary) und importiert sie in die Datenbank. Anders als `db import` wird hier gestreamt: die Quelldateien sind GB-groß, aber es werden nur die ersten `max_words` passenden Zeilen gelesen — der Rest der Datei wird nie heruntergeladen.

#### Syntax

```
spoor db fetch [OPTIONS]
```

#### Optionen

| Option | Wert | Beschreibung |
|--------|------|-------------|
| `--file <FILE>` | Path | Pfad zur Quellen-Konfiguration. Standardwert: `sources.yaml` |
| `--only <IDS>` | String | Komma-getrennte Liste von Source-IDs (z. B. `kaikki-de,kaikki-la`). Ohne Angabe werden alle Quellen aus der Datei abgerufen. |
| `--limit <N>` | usize | Überschreibt `max_words` für alle ausgewählten Quellen (z. B. für schnelle Tests). |

#### Ablauf

- Jede Quelle wird in einem eigenen Thread parallel heruntergeladen und geparst (Streaming, keine Zwischenspeicherung der ganzen Datei).
- Ein einziger Thread (der Haupt-Thread) schreibt in die SQLite-Datenbank — pro eintreffendem Batch (100 Wörter) eine Transaktion. So bleibt die "eine Verbindung, ein Schreiber"-Regel der Datenbank gewahrt, auch bei parallelen Downloads.
- Scheitert eine Quelle (Netzwerkfehler, Timeout, ungültige URL), werden die anderen Quellen davon nicht beeinträchtigt. Die fehlgeschlagene Quelle wird mit `✖` und Fehlermeldung markiert; der Befehl selbst schlägt nicht fehl.

#### Beispiel: Eine Quelle mit Limit (schneller Testlauf)

```bash
spoor db fetch --only kaikki-la --limit 50
```

Ausgabe (Live-Update, docker-compose-artig — eine Zeile pro Quelle):
```
[+] Fetching 1 sources
⠿ kaikki-la     2.1 MB · 340 Woerter · 120 uebersprungen
✔ kaikki-la     50 Woerter importiert (2.4 MB gelesen)
Imported 50 words from 1 sources.
```

#### Beispiel: Alle konfigurierten Quellen

```bash
spoor db fetch --limit 100
```

Ausgabe:
```
[+] Fetching 3 sources
⠿ kaikki-de     1.8 MB · 210 Woerter · 55 uebersprungen
⠿ kaikki-en     2.4 MB · 300 Woerter · 90 uebersprungen
✔ kaikki-la     100 Woerter importiert (2.4 MB gelesen)
✔ kaikki-de     100 Woerter importiert (3.1 MB gelesen)
✔ kaikki-en     100 Woerter importiert (3.9 MB gelesen)
Imported 300 words from 3 sources.
```

Jede Quellenzeile aktualisiert sich live (Spinner, gelesene Datenmenge, akzeptierte/übersprungene Wörter) und endet mit `✔` (Erfolg) oder `✖` (Fehler mit Meldung). Wenn stdout kein Terminal ist (z. B. Umleitung in eine Datei), unterdrückt `indicatif` die Live-Anzeige automatisch.

Details zum Quellenformat siehe `docs/reference/data-model.md` (Abschnitt "sources.yaml").

### db info — Datenbankstatistiken

Zeigt grundlegende Statistiken über die importierten Daten.

#### Syntax

```
spoor db info
```

#### Beispiel

```bash
spoor db info
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
spoor gen --count 3
# Ausgabe:
# seed=12345678
# Name A
# Name B
# Name C

# Lauf 2: Seed notieren und später reproduzieren
spoor gen --seed 12345678 --count 3
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

| Fehler | Ursache | Naechster Schritt |
|--------|--------|--------|
| `Keine Treffer fuer '<query>'` + Naechster Schritt: `es ohne --systems zu versuchen` | Query hat mit aktuellem System-Filter keine Treffer | `--systems`-Filter entfernen und erneut suchen |
| `Keine Treffer fuer '<query>'` + Naechster Schritt: `spoor db fetch --limit 1000` | Query hat generell keine Treffer im gesamten Bestand | Mit `spoor db fetch --limit 1000` mehr Woerter laden oder andere Schluesselbegriffe ausprobieren |
| `no words available - import data first` | Datenbank ist vollstaendig leer | `spoor db import data/words.csv` ausführen |
| `Datei nicht gefunden: <path>` | `spoor db import` erhielt einen ungültigen Dateipfad | Pfad überprüfen und korrekt angeben |
| `Quellendatei nicht gefunden: sources.yaml` | `sources.yaml` für `spoor db fetch` nicht vorhanden | Datei im Repository anlegen oder mit `--file <path>` korrekten Pfad angeben |
| `Failed to read config file` | `config.toml` nicht gefunden (wenn explizit mit `--config` angegeben) | Datei erstellen oder Pfad überprüfen. Hinweis: ohne `--config` nutzt spoor eingebaute Defaults |
| `Unknown placeholder: {foo}` | Ungültiger Platzhalter im Template | Nur `{prefix}`, `{word}`, `{suffix_adj}`, `{suffix}` verwenden |
| `only N unique names were possible` | Zu wenig Wörter für --count | `--count` reduzieren oder mehr Wörter importieren |
| `✖` bei `db fetch` mit Fehlermeldung | Netzwerkfehler, Timeout oder ungültige URL für eine Quelle | Internetverbindung prüfen, URL in `sources.yaml` korrigieren; andere Quellen sind davon nicht betroffen |

