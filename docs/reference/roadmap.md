# Roadmap — Name Generator

Phasen-Übersicht und aktueller Status.

## North Star (Leitgedanke)

Siehe `docs/NORTH_STAR.md` für die ausführliche Vision.

**Kurz**: Ein Tool, das aus einem Anwendungsfall einen **einzelnen passenden Namen** findet — mit nachvollziehbarer Herkunft, nicht nur Zufall.

---

## Phasenplan

### Phase 0: Rust-Port mit reproduzierbarer Generierung ✅ FERTIG

**Zeitraum**: Abgeschlossen

**Ziele**:
- Rust-Neufassung des Node.js-Prototyps
- CLI mit Subcommand-Hierarchie (gen, list, db)
- Deterministische Generierung über Seed
- SQLite-Datenbank statt CSV-basierte Laufzeit
- Template-Engine mit Platzhaltern {prefix}, {word}, {suffix_adj}, {suffix}
- Duplikat-Vermeidung
- Reproduzierbare Tests (deterministic seed)

**Deliverables**:
- Binary `target/release/name-generator.exe`
- `config.toml` mit Wahrscheinlichkeitskonfiguration
- `src/` mit modularer Architektur (main, cli, config, db, generator)
- Tests in `tests/`

**Status**: ✅ FERTIG
- CLI funktioniert vollständig
- Seed-Reproduzierbarkeit verifiziert
- Datenbankimport, Listung, Generierung arbeiten

**Abhängigkeiten**: Keine

---

### Phase 1: CLI-Subcommand-Hierarchie und Referenzdokumentation ✅ FERTIG

**Zeitraum**: Mit diesem Commit

**Ziele**:
- CLI-Struktur aufräumen: `gen` / `list {systems|languages|classes|words}` / `db {import|info}`
- Vollständige Kommandoreferenz in `docs/reference/cli.md` (mit geprüften Beispielen)
- Architektur-Dokumentation in `docs/reference/architecture.md`
- Datenmodell-Dokumentation in `docs/reference/data-model.md` (CSV, SQLite, Duplikate)
- Diesen Roadmap
- `docs/AGENT_PROMPT.md` und `docs/PROJECT_BRIEF.md` löschen (veraltet, Inhalte → roadmap/data-model)
- `README.md` anpassen: neue Subcommand-Struktur, Verweis auf `docs/reference/`

**Deliverables**:
- `docs/reference/cli.md` (Kommandoreferenz, deutsche Erklärungen, echte Beispiele)
- `docs/reference/architecture.md` (Module, Datenflüsse, Designprinzipien)
- `docs/reference/data-model.md` (CSV-Format, SQLite-Schema, Duplikate, Zukünftiges)
- `docs/reference/roadmap.md` (dieses Dokument)
- Aktualisiertes `README.md`
- Gelöschte veraltete Dateien

**Status**: ✅ FERTIG

**Abhängigkeiten**: Phase 0

---

### Phase 2: Etymologie und Herkunftssprache (Datenfundament für Reverse-Lookup) ✅ FERTIG

**Zeitraum**: Mit diesem Commit

**Ziele**:
- CSV-Format erweitern: neue Spalten `etymology` (Kurzbeschreibung) und `origin_lang` (Ursprungssprache)
- SQLite-Schema aktualisieren: neue Spalten (nullable)
- Import-Pipeline: CSV-Spalten → DB (rückwärtskompatibel)
- Datenbasis füllen: Etymologien für bestehende Einträge recherchieren und hinzufügen
- `docs/reference/data-model.md` aktualisieren
- **Kein Wiktionary-Dump-Import** noch (bewusst verschoben auf Phase 4)

**Deliverables**:
- ✅ Aktualisierte `data/words.csv` mit `etymology` und `origin_lang` (alle 77 Einträge)
- ✅ Schema-Migration in `db/mod.rs`
- ✅ Alle Einträge mit Etymologien gefüllt (100% des Bestands)
- ✅ Dokumentation der neuen Spalten in data-model.md

**Status**: ✅ FERTIG

**Abhängigkeiten**: Phase 0, Phase 1

**Hinweis**: Wiktionary-Integration bewusst NICHT in Phase 2. Phase 2 ist Datenkuration; Phase 4 ist Automation.

---

### Phase 3: Reverse-Lookup v1 (Semantische Suche nach Anwendungsfall) ✅ FERTIG

**Zeitraum**: Mit diesem Commit

**Ziele**:
- Neuer Command: `find "<beschreibung>" [--count N] [--explain] [--systems <systems>]`
- Keyword-basiertes Scoring (keine Embeddings):
  - Nutzer-Input tokenisieren
  - Stopwörter entfernen (DE/EN)
  - Treffer-Gewichtung: `word` > `tags` > `system` > `etymology`
  - Ranking nach Relevanz
- Ausgabe:
  - Standard: ein Wort (Default: count=1, "North Star")
  - `--count N`: N beste Wörter
  - `--explain`: Etymologie + Begründung (warum dieses Wort passt)
  - `--systems <list>`: Filter auf Systeme
- Keine Embeddings noch (bleibt für Phase 4)

**Deliverables**:
- ✅ `src/lookup/mod.rs` Modul mit:
  - `tokenize()` — Keyword-Tokenisierung + Stoppwörter-Filter
  - `score_record()` — Relevanz-Bewertung pro Datensatz
  - `rank()` — Deterministische Sortierung
  - `explain()` — Deutsche Etymologie-Ausgabe
- ✅ CLI-Command `find <QUERY> [--count N] [--explain] [--systems S] [--format text|json]`
- ✅ Beispiele in `docs/reference/cli.md` (find-Command mit 3 Beispielen)
- ✅ Tests in `tests/lookup_test.rs` (6 Tests: tokenize, rank-precedence, determinism, tiebreak, explain, no-match)
- ✅ Architektur-Dokumentation in `docs/reference/architecture.md`
- ✅ Scoring-Logik: word (5.0) > tag (3.0) > system (2.0) > etymology (1.0)

**Status**: ✅ FERTIG
- `cargo test` — 14 Tests grün (8 integration + 6 lookup)
- `cargo build` — Zero Warnings
- `find "sky thunder king"` → "zeus"
- `find "Werkzeug für Wald und Baum" --count 3 --explain` → 2–3 Ergebnisse mit Etymologie
- `find "xyzzy quux"` → No matches, exit code 1
- Determinismus verifiziert: identische Ausgabe bei zwei Läufen

**Abhängigkeiten**: Phase 0, Phase 2

**Beispiel-Workflow**:
```bash
name-generator find "sky thunder king"
# Ausgabe:
# zeus

name-generator find "Werkzeug für Wald und Baum" --count 3 --explain
# Ausgabe:
# wald — ahd. wald, germ. *walþuz (goh) · System: nature · Treffer: wald (word), wald (etymology)
# silvan — lat. silva 'Wald' (la) · System: nature · Treffer: wald (etymology)
```

---

### Phase 4: Semantik-Upgrade (optional, später)

**Zeitraum**: Optional, nach Phase 3

**Ziele**:
- Lokale Embeddings (z. B. via ONNX, Sentence-Transformers-Modell)
- Hinter derselben `find`-Oberfläche wie Phase 3
- Bessere semantische Matches (statt nur Keyword-Matching)
- **Optional**: Wiktionary-Dump-Import-Pipeline (großes Datenvolumen)

**Deliverables**:
- `src/lookup/embed.rs` — ONNX/Embeddings-Wrapper
- Embedding-Modell (klein, lokal) oder Download-On-First-Run
- Scoring-Upgrade: Cosine-Similarity statt Keyword-Matching
- Dokumentation der Anforderungen (Modellgröße, RAM-Nutzung)

**Status**: ⏳ OPTIONAL

**Abhängigkeiten**: Phase 3

**Entscheidung ausstehend**: 
- Größe des Binary (mit Embedding-Modell?)
- Download-on-First-Run vs. Bundle?
- Welches Modell?

**Hinweis**: Nicht erzwungen, wenn die Keyword-Suche (Phase 3) ausreicht.

---

## Datenquellen und Erweiterung

### Primär: CSV-basierter Import (Phase 0+)

- **Format**: `data/words.csv` mit Spalten (word, language, word_class, system, tags, seed_weight, source, etymology, origin_lang)
- **Nutzen**: Kleine, kurierte Listen (100–1000 Wörter pro System)
- **Quellen**:
  - Manuell curated (Mythologie, Natur, Handwerk)
  - GitHub-Wordlists (`kkrypt0nn/wordlists` u.ä.)

**Workflow**: CSV erstellen → `name-generator db import` → Database

**Backward-Kompatibilität**: 7-spaltige CSVs werden mit leeren Etymologien importiert.

### Sekundär: Etymologie-Erweiterung (Phase 2)

- **Format**: Spalten `etymology` und `origin_lang` in CSV
- **Nutzen**: Für `find --explain` in Phase 3
- **Quellen**: Wiktionary-Lektüre, Etymologie-Datenbanken manuell

### Tertiär: Wiktionary-Dump-Import (Phase 4, optional)

- **Werkzeug**: `wiktextract` (Python) zum Parsen von XML-Dumps
- **Nutzen**: Große, automatisierte Datenmengen (10.000+ Wörter pro Sprache)
- **Pipeline**: Dump → JSON (via wiktextract) → DB (via Rust-Importer)
- **Entscheidung**: Nur wenn Phase 3 (Keyword-Suche) bewährt

**Hinweis**: Wiktionary-Integration bewusst NICHT in Phase 2. Das ist Data-Engineering, nicht Datenwissenschaft.

---

## Erfolgs-Kriterien pro Phase

### Phase 0 ✅

- ✅ Binary läuft ohne Abhängigkeiten
- ✅ Seed 42 → identische Ausgabe mehrfach
- ✅ CSV-Import funktioniert
- ✅ Vier Subcommands (gen, list, db, help) funktionieren

### Phase 1 ✅

- ✅ Vollständige CLI-Referenz mit echten Beispielen
- ✅ Architektur-Dokumentation
- ✅ Datenmodell-Dokumentation
- ✅ README angepasst

### Phase 2 ✅

- ✅ Alle 77 Einträge haben `etymology` und `origin_lang`
- ✅ Schema-Migration läuft rückwärtskompatibel
- ✅ CSV und DB importieren korrekt

### Phase 3 ✅

- ✅ `find "sky thunder king"` gibt "zeus" zurück
- ✅ `find ... --explain` zeigt Etymologie + System + Treffer
- ✅ Scoring-Tests vorhanden (tokenize, rank, determinism, tiebreak)
- ✅ Determinismus verifiziert (identische Ausgabe bei zwei Läufen)
- ✅ `cargo test` — 14 grün (8 integration + 6 lookup)
- ✅ `cargo build` — Zero Warnings

### Phase 4 (optional)

- Embedding-basierte Suche funktioniert
- Binary-Größe akzeptabel
- Semantic-Search-Tests zeigen 80%+ Precision

---

## Bekannte Limitierungen (Phase 0–1)

| Limitation | Grund | Behebung |
|-----------|-------|----------|
| Nur 77 Wörter | Kleine Datenbasis | Phase 2 curation, Phase 4 Wiktionary |
| Nur Keyword-Matching | Keine semantische Suche | Phase 3, dann Phase 4 Embeddings |
| Template-Struktur fest | Nur 4 Platzhalter | Zukünftig erweiterbar (kein Breaking Change) |
| Keine Gewichtung | `seed_weight` ignoriert | Phase 2+ (für Sampling-Wahrscheinlichkeit) |
| Keine Etymologie-Ausgabe | Spalten noch nicht da | Phase 2 (CSV erweitern), Phase 3 (find --explain) |

---

## Technologie-Entscheidungen

### Sprache: Rust

- **Pro**: Binary ohne Runtime, schnell, deterministic (ChaCha8)
- **Contra**: Komplexer für Anfänger
- **Stand**: Nicht zu ändern (Phase 0 abgeschlossen)

### Datenbank: SQLite

- **Pro**: Lokal, keine Server, einfaches Schema
- **Contra**: Nicht ideal für 1M+ Einträge (aber für Phase 0–3 ausreichend)
- **Stand**: Nicht zu ändern

### RNG: ChaCha8

- **Pro**: Deterministisch, schnell, kryptographisch solide
- **Contra**: Overkill für Name-Generation (aber sicher)
- **Stand**: Nicht zu ändern (funktioniert)

### Lookup: Keyword → Embedding (Phase 3 → 4)

- **Phase 3**: Tokenizerung + Stopwörter (Rust-Standard)
- **Phase 4**: ONNX-Modell (lokal, kein API-Call)
- **Grund**: Datenschutz, Geschwindigkeit, Offline-Betrieb

---

## Offen für Feedback

- Phase-Reihenfolge änderbar (z. B. Phase 4 früher, wenn Embedding sinnvoll)
- Neue Datenquellen willkommen (Links/Vorschläge)
- Template-Erweiterung (z. B. {etymon}, {language}) bei Bedarf
- Lokalisierung der CLI (Deutsch/Englisch-Mischung vs. vollständig eine Sprache)

