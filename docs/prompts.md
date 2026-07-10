# Prompt-Katalog & Benchmark

Alle Eingaben, die spoor versteht — als Kopiervorlagen. Der zweite Teil ist
die feste Benchmark-Suite: identische Queries, die nach jeder Änderung am
Ranking erneut laufen (`scripts/benchmark.ps1`), damit Ergebnisveränderungen
über Diffs sichtbar werden.

## find — Bedeutung → Name

```powershell
# Der Kernfall: ein Wort zu einer Beschreibung (online, wenn Netz da)
spoor find "CLI die Logs von verteilten Systemen synchronisiert"

# Mehrere Vorschläge mit Begründung (Spur + Wurzel)
spoor find "Werkzeug für Wald und Baum" --count 5 --explain

# Einzelwort: liefert Assoziationen, NIE das Wort selbst (Anti-Echo)
spoor find "Baum" --count 3 --explain

# Echo bewusst zulassen (zeigt auch 'Baum' selbst)
spoor find "Baum" --allow-echo --count 3

# Nur lokale Suche, kein Netzzugriff (deterministisch, benchmark-tauglich)
spoor find "Wasser Licht" --offline --count 3 --explain

# Online erzwingen (Fehler statt Fallback, wenn kein Netz/keine Config)
spoor find "synchronize logs distributed" --online --count 3

# Auf Systeme einschränken (Brücke nutzt trotzdem den Gesamtbestand)
spoor find "himmel donner könig" --systems myth_greek,wiktionary_grc --explain

# Maschinenlesbar (mode, display, translit, matched, score)
spoor find "light" --format json --count 3

# Stichwortmix Deutsch/Englisch — die Glossenbrücke übersetzt automatisch
spoor find "audio dispatch senden kommunikation genuss spiel musik sonne" --count 9 --explain
```

## gen — reproduzierbare Namensgenerierung

```powershell
# Ein Name, zufälliger Seed (Seed wird ausgegeben → wiederverwendbar)
spoor gen

# Reproduzierbar: gleicher Seed = gleiche Namen, immer
spoor gen --seed 42 --count 5

# Nur aus bestimmten Systemen schöpfen
spoor gen --seed 7 --count 3 --systems nature,myth_greek

# Eigene Struktur per Template
spoor gen --seed 42 --template "The {word} of {suffix_adj} {suffix}"
spoor gen --seed 42 --template "Only {word}"

# JSON-Ausgabe
spoor gen --seed 42 --count 3 --format json
```

## list — Bestand erkunden

```powershell
spoor list systems              # Systeme mit Wortzahl
spoor list languages            # Sprachen mit Wortzahl
spoor list classes              # Wortklassen mit Wortzahl
spoor list words --system nature
spoor list words --language grc --system wiktionary_grc
```

## db — Bestand pflegen

```powershell
spoor db info                                   # Statistik gesamt
spoor db import data/words.csv                  # CSV importieren
spoor db fetch                                  # Alle Quellen aus sources.yaml (bis 20k/Quelle)
spoor db fetch --only kaikki-la --limit 500     # Eine Quelle, gekappt
spoor db fetch --only kaikki-he,kaikki-grc,kaikki-el   # Nicht-Latein-Quellen (Romanisierungen)
```

## Sonstiges

```powershell
spoor                       # Status-Screen (Bestand, Einstieg)
spoor help                  # Alle Kommandos
spoor find --help           # Optionen eines Kommandos
spoor --config pfad/zur/config.toml find "..."   # Alternative Config/DB
```

---

# Benchmark-Suite

**Zweck:** dieselben Eingaben nach jeder Ranking-/Daten-Änderung ausführen
und die Ausgaben diffen. Deshalb läuft alles mit `--offline` (Datamuse-
Antworten variieren) und festen Seeds — die Ausgabe ist vollständig
deterministisch *für einen gegebenen Datenbestand*. Der Kopf jedes Laufs
protokolliert die Bestandsgröße, damit Diffs einordbar sind.

**Ausführen:**

```powershell
./scripts/benchmark.ps1
# Ergebnis: benchmarks/latest.txt (+ zeitgestempelte Kopie benchmarks/run-<datum>.txt)
# Vergleich mit vorherigem Lauf:
git diff --no-index benchmarks/run-<alt>.txt benchmarks/latest.txt
```

**Die Fälle** (Erwartung in Klammern — bewusst grob, der Diff ist das Messinstrument):

| # | Query | Prüft |
|---|-------|-------|
| B01 | `Baum` | Anti-Echo + Brücke de→en→la/grc (arbor-Familie, nie „Baum") |
| B02 | `Werkzeug für Wald und Baum` | Stopwörter, Mehrwort-Query, Brücke |
| B03 | `CLI die Logs von verteilten Systemen synchronisiert` | North-Star-Kernfall |
| B04 | `Wasser Licht` | Zwei Konzepte kombiniert (fons/Born-Klasse) |
| B05 | `sky thunder king` | Direkte englische Gloss-Treffer, kuratierte Wörter |
| B06 | `track meaning to its origin` | Englische Konzepte, Etymologie-Feld |
| B07 | `audio dispatch senden kommunikation genuss spiel musik sonne` | Breite Mischquery (Nutzer-Testfall) |
| B08 | `schwarz logs lesen forensisch skalpell schneiden lupe suchen präzise` | Breite Mischquery (Nutzer-Testfall) |
| B09 | `weisheit erkenntnis lehre` | grc/he-Treffer + Transliteration |
| B10 | `feuer schmiede handwerk` | craft/Natur-Systeme |
| B11 | `track hunt trace search` | Assoziations-Hop über Nexus-Kanten (N1) |
| B12 | `himmel donner goetter blitz` | Mythologie-Quelle + alte Sprachen |
| G01 | `gen --seed 42 --count 5` | Generator-Determinismus |
| G02 | `gen --seed 42 --count 3 --systems nature` | Systemfilter im Generator |

**Lesehilfe für Diffs:** Ändert sich B01–B10 nach einem reinen Fetch
(mehr Daten), ist das erwartet — die Kopfzeile zeigt den neuen Bestand.
Ändert sich etwas nach einer reinen Code-Änderung bei gleichem Bestand,
ist das die Wirkung der Änderung (gewollt oder Regression).
