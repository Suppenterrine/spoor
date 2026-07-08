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
cargo test          # 17 Tests, u.a. Determinismus, Migration, Lookup-Ranking
cargo build --release
```

Der frühere Node.js-Prototyp wurde durch diese Rust-Implementierung ersetzt und aus dem Repository entfernt (in der Git-History weiterhin verfügbar).
