# spoor

> „SPOOR — follow meaning to the name.“

Ein CLI-Werkzeug, das aus der Bedeutung eines Anwendungsfalls den passenden Namen findet — als einzelnes Wort, mit einer Spur (engl./ndl. *spoor*: die Fährte), die bis zu seiner Herkunft zurückverfolgbar ist. Dazu ein seed-reproduzierbarer Zufallsgenerator für thematische Namen.

Vision und Leitplanken: [docs/NORTH_STAR.md](docs/NORTH_STAR.md) · Zum Namen: [docs/rename-project.md](docs/rename-project.md)

```console
$ spoor find "sky thunder king" --explain
zeus — griech. Zeus, idg. *dyeus 'Himmel, Tag' (grc) · System: myth_greek · Treffer: sky (tag), thunder (tag), king (tag)

$ spoor gen --seed 42 --count 3
Wandering atlas of Essenz
Heilig silvan
The Iron apex of the verborgen Dawn
```

## Installation

Voraussetzung: [Rust-Toolchain](https://rustup.rs/) (cargo).

```bash
git clone https://github.com/Suppenterrine/spoor.git
cd spoor
cargo build --release
```

Das Binary liegt danach unter `target/release/spoor` (Windows: `spoor.exe`, ~5 MB, keine Laufzeitabhängigkeiten).

**Das Binary funktioniert sofort nach dem Download — ohne zusätzliche Konfiguration oder Datenbankinitialisierung.** Die Basisdaten sind eingebettet und werden beim ersten Aufruf automatisch im Nutzer-Datenverzeichnis angelegt.

## Schnellstart

Das Binary ist sofort einsatzfähig, ohne vorbereitende Schritte:

```bash
# 1. Reverse-Lookup: Einen Namen für einen Anwendungsfall finden
spoor find "forest tree" --explain
# Ausgabe beim 1. Aufruf: Initialized word database... [dann: silvan — ...]
# Ausgabe beim 2. Aufruf: silvan — ... (ohne Init-Meldung)

# 2. Zufällige Namen generieren — reproduzierbar über den Seed
spoor gen --seed 42 --count 5

# 3. Datenbankstatistik
spoor db info
```

Im Repository können die Basisdaten erweitert werden:

```bash
# Optional: Wortdatenbank aus erweiterter CSV importieren
spoor db import data/words.csv

# Optional: Wörter von Online-Wörterbüchern laden
spoor db fetch --only kaikki-de --limit 100
```

## Arbeitsmodell

spoor funktioniert nach zwei Fokusmodalitäten:

**1. Finden (spoor find)**  
Der Northstar-Weg: Man beschreibt eine Bedeutung oder einen Anwendungsfall in Schlüsselwörtern, spoor durchsucht den lokalen Wortbestand und findet *ein passendes Wort* mit seiner etymologischen Herkunft. Dies ist Reverse-Lookup.

```console
$ spoor find "sky thunder king" --explain
zeus — griech. Zeus, idg. *dyeus 'Himmel, Tag' (grc) · System: myth_greek · Treffer: sky (tag), thunder (tag), king (tag)
```

**2. Generieren (spoor gen)**  
Seed-deterministische Erzeugung von Namenskombinationen aus dem Wortbestand. Ein fester Seed (`--seed 42`) ergibt immer die gleiche Abfolge; ohne Seed wird einer zufällig generiert und ausgegeben. Basis sind Wortklassen (Prefix, Noun, Suffix) und konfigurierbare Wahrscheinlichkeiten.

```console
$ spoor gen --seed 42 --count 3
Wandering atlas of Essenz
Heilig silvan
The Iron apex of the verborgen Dawn
```

**Der Datenkreislauf**  
- **Eingebettete Basisdaten (77 Worte)**: Werden beim ersten Aufruf automatisch aus dem Binary ins System-Datenbankverzeichnis importiert. Man kann sofort `find` und `gen` nutzen.
- **Optionale Erweiterung**: `spoor db fetch --limit 1000` lädt weitere Wörter von Online-Wörterbüchern (konfiguriert in `sources.yaml`).
- **Arbeitsfeld**: `find` und `gen` operieren auf dem aktuellen Bestand. Mit `spoor list languages|systems` kann man den Datenbestand erkunden.

Eine typische Session (frische Installation, 77 eingebettete Basiswörter — nach `db fetch` zeigen Zahlen und Treffer den gewachsenen Bestand):

```console
$ spoor
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

$ spoor find "werkzeug fuer wald und baum" --explain
wald — ahd. wald, germ. *walþuz (goh) · System: nature · Treffer: wald (word), wald (etymology)

$ spoor gen --seed 42 --count 1
Wandering atlas of Essenz
```

## Kommandos

| Kommando | Zweck |
| --- | --- |
| `gen` | Namen generieren (`--seed`, `--count`, `--systems`, `--template`, `--format`) |
| `find <beschreibung>` | Reverse-Lookup: passendes Wort zum Anwendungsfall (`--count`, `--systems`, `--explain`, `--format`) |
| `list systems\|languages\|classes\|words` | Datenbankinhalt erkunden |
| `db import <csv>` | Wortlisten importieren (streamend, dedupliziert) |
| `db info` | Statistik nach Sprache und System |
| `help [kommando]` | Hilfe, auch ohne Dashes |

Zwei Eigenschaften gelten überall:

- **Reproduzierbarkeit**: Ohne `--seed` wird ein Seed erzeugt und als `seed=<n>` ausgegeben; derselbe Seed liefert exakt denselben Lauf.
- **Herkunft sichtbar**: Jeder `find`-Treffer nennt mit `--explain` seine Etymologie und die Felder, über die er gefunden wurde.

Vollständige Referenz: [docs/reference/cli.md](docs/reference/cli.md)

## Konfiguration

`config.toml` steuert Wahrscheinlichkeiten und Aufbau der Generierung (Prefix/Suffix/Artikel, Separator, Fillword) sowie den Datenbankpfad. Abweichender Pfad: globales Flag `--config <pfad>`.

## Eigene Wortdaten

`data/words.csv` ist der mitgelieferte Startdatensatz (77 kuratierte Wörter, EN/DE/LA, mit Etymologien). Format und Wortklassen: [docs/reference/data-model.md](docs/reference/data-model.md). Die Datenbank `data/words.db` ist generiert und nicht versioniert — nach dem Klonen einmal `db import` ausführen.

## Dokumentation

- [docs/reference/cli.md](docs/reference/cli.md) — Kommandoreferenz
- [docs/reference/architecture.md](docs/reference/architecture.md) — Module, Datenfluss, Designprinzipien
- [docs/reference/data-model.md](docs/reference/data-model.md) — CSV-Format und Datenbankschema
- [docs/reference/roadmap.md](docs/reference/roadmap.md) — Phasenplan und Status (Reverse-Lookup v1 ist umgesetzt; größere Datenmengen und semantisches Matching sind Phase 4)

## Entwicklung

```bash
cargo test          # 49 Tests, u.a. Determinismus, Migration, Lookup-Ranking, CLI-Integration
cargo build --release
```

Der frühere Node.js-Prototyp wurde durch diese Rust-Implementierung ersetzt und aus dem Repository entfernt (in der Git-History weiterhin verfügbar).
