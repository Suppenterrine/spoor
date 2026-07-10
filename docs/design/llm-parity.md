# Design: LLM-Parität ohne LLM

Datum: 2026-07-10
Status-Update (2026-07-10, abends): **N1** (Wiktionary-Kanten → edges-Tabelle
+ Assoziations-Hop, ~295k Kanten im Vollbestand), **N2** (Register-Spalte,
Boost ×1.15, `--register`-Flag), **N3** (9 alte Sprachen: sa, non, got, egy,
akk, sux, nci, yua, qu) und **N5.1** (mythology-json-Backend,
greek-mythology-data) sind umgesetzt und im Benchmark (B11/B12). Offen:
N4 (ConceptNet-Pack), N5.2/5.3 (MANTO, Wikidata), N6 (Formfilter,
Begründungssatz), E1 (Embeddings-Experiment).
Scoring-Lektion aus B11: ein Glossen-Token liefert pro Wort nur einmal
Evidenz — sonst stapeln präfix-verwandte Konzepte Phantom-Score.
Basis: `docs/research/semantic-gap.md` (Zerlegung der LLM-Leistung in 5 Teilschritte)
Prämissen: deterministisch, offline-first, Binary klein, Herkunft sichtbar.
Priorität laut Produktentscheidung: **Assoziationen > Metaphern/Poesie >
Formfilter/Begründung**. Embeddings sind eine Experimentierspur, nicht die
Hoffnung.

## Wo wir stehen (Teilschritte aus dem Research-Dokument)

| # | LLM-Teilschritt | Stand | Lücke |
|---|-----------------|-------|-------|
| 1 | Konzeptextraktion | ✅ tokenize + Konzeptbrücke | Mehrwort-Konzepte, Query-Gewichtung |
| 2 | **Assoziative Expansion mit Abstand** | ⚠️ nur Datamuse `ml`/`rel_trg` (online) | offline-Assoziationen, Metaphernsprung |
| 3 | Cross-Sprach-Projektion | ✅ Glossenbrücke | mehr Sprachen (trivial: sources.yaml) |
| 4 | Formfilter (Silben, Klang) | ❌ | Silbenzähler, Längen-/Klangfilter |
| 5 | Begründung aus Etymologie | ⚠️ Spur + Wurzel roh | ein deterministischer Begründungssatz |

Die eigentliche Lücke ist #2 — und innerhalb von #2 genau das, was dich am
meisten interessiert: **Assoziation, Metapher, Poesie**. Der Rest ist
Handwerk.

## Der Kernbefund: Assoziationen sind Kanten, keine Vektoren

Was das LLM beim „spoor"-Beispiel geleistet hat („Namen finden" → *Fährte
lesen*, Jägersprache), ist ein Spaziergang über einen Assoziationsgraphen:
Bedeutung → Symbol → Nachbardomäne. Das können wir deterministisch
nachbauen, wenn wir die Kanten haben. Deshalb ist das Zielbild ein
**Kanten-Nexus** in SQLite neben der `words`-Tabelle:

```sql
CREATE TABLE edges (
    src    TEXT NOT NULL,   -- Konzept (lowercased, en als Interlingua)
    rel    TEXT NOT NULL,   -- related | synonym | derived | symbol_of | used_for | ...
    dst    TEXT NOT NULL,
    weight REAL DEFAULT 1.0,
    source TEXT             -- wiktextract | conceptnet | myth_greek | ...
);
```

Stufe A des Lookups (build_concepts) läuft dann nicht nur über die
Glossenbrücke, sondern zusätzlich 1–2 Hops über `edges`, mit Gewichtszerfall
pro Hop und **vollständig sichtbarer Spur**:
`fährte ← related ← spur ← symbol_of ← suche`. Kein Blackbox-Score — jede
Assoziation hat eine benennbare Kante. Das ist der Unterschied zu
Embeddings und der Grund, warum der Graph die Hauptspur ist.

## Maßnahmen (konkret, geordnet)

### N1 — Wiktionary-eigene Kanten ernten (S, sofort möglich)

Die kaikki-Zeilen, die wir schon streamen, enthalten ungenutzte Felder:
`synonyms`, `related`, `derived`, `antonyms`, `coordinate_terms`. Das sind
fertige Assoziationskanten aus derselben Quelle — kein neuer Download, nur
Parser + `edges`-Tabelle. Sofortiger Assoziationsgewinn, offline,
deterministisch.

**Braucht:** `edges`-Tabelle, Parser-Erweiterung (~40 Zeilen), Hop-Expansion
in build_concepts, Label `(assoziation)` in der Spur.

### N2 — Metaphern & Poesie über Register-Tags (S–M, größter Hebel für deine Priorität)

kaikki-Senses tragen Register-Tags, die wir derzeit wegwerfen:
`figuratively`, `poetic`, `literary`, `archaic`, `dated`. Genau dort wohnen
Metaphern: eine figurative Glosse IST der Metaphernsprung des Wörterbuchs
(z. B. *Faden* – „figuratively: roter Faden, Zusammenhang").

**Plan:**
- Neue Spalte `registers` (z. B. `figurative,poetic`), beim Fetch aus den
  Sense-Tags gefüllt; die zugehörige Glosse bleibt (nur *Buchhaltung* wird
  weiter gefiltert, nicht *Bildsprache*).
- Scoring: Treffer auf figurativer Glosse bekommt eigenes Label
  `(metapher)` und einen Bonus; `--register poetic|figurative` filtert bzw.
  boostet gezielt.
- Poesie-Register (`poetic`, `literary`) als seed_weight-Bonus: poetische
  Wörter sind bessere Namen.

**Braucht:** Spalte + Fetch-Erweiterung, Scoring-Label, CLI-Flag.

### N3 — Ursprüngliche Sprachen erweitern (S, reine Konfiguration)

kaikki hat alle gewünschten „ganz alten" Sprachen als fertige JSONL-Dumps
(gleicher Parser!): **Yucatec Maya** (767 Senses), **Classical Nahuatl**
(5 130), **Sumerisch** (3 162), **Akkadisch** (2 205), **Sanskrit** (30 069),
**Altnordisch** (15 550), **Ägyptisch** (8 523), **Gotisch** (25 142),
**Quechua** (3 808). Maya/Nahuatl/Quechua sind lateinschriftlich;
Sumerisch/Akkadisch/Ägyptisch haben Romanisierungen im Dump.

**Braucht:** nur sources.yaml-Einträge + origin_factor-Erweiterung.
**Achtung:** kaikki markiert die per-Sprache-JSONL-URLs als „deprecated";
die Sprach-Hauptseiten bleiben die maßgebliche Quelle für die aktuellen
Links — beim nächsten URL-Bruch dort nachsehen (ggf. .gz-Variante, unser
Fetcher kann das schon).

### N4 — ConceptNet als Offline-Kantenpack (M, der Assoziations-Hauptgang)

ConceptNet ist mehrsprachig, CC-lizenziert und hat genau die Relationen für
den Metaphernsprung: `RelatedTo`, `SymbolOf`, `UsedFor`, `Causes`,
`HasProperty`, `MadeOf`, `AtLocation`. Die REST-API (api.conceptnet.io) war
2025 wiederholt instabil (502-Ausfälle, Serverumzug, Rate-Limits) →
**Dump statt API**: assertions-CSV (gz) streamen — dieselbe
Streaming-Infrastruktur wie kaikki —, auf en/de und die relevanten
Relationen filtern, in `edges` schreiben. Geschätzt wenige hunderttausend
Kanten → einstellige MB in SQLite.

**Braucht:** neuen Backend-Typ `conceptnet-csv` in sources.yaml, Filterliste,
Hop-Expansion (aus N1 schon da). Als optionales Pack (Leitplanke 4).

### N5 — Mythologie-Quellen (M)

Recherche-Ergebnis, drei brauchbare Ebenen:
1. **greek-mythology-data** (GitHub, JSON, npm-Paket): flache Listen von
   Göttern/Kreaturen mit Kategorien — einfachster Start, ein
   `mythology-json`-Backend, Wörter → system `myth_greek`, Kategorien →
   Kanten (`zeus —domain→ sky,thunder`).
2. **MANTO** (api.manto.unh.edu, auch CSV): wissenschaftliches Datenset
   griechisch-römischer Mythen — Personen, Orte, Objekte, Ereignisse mit
   stabilen URIs. Reicher, aber mehr Mapping-Aufwand.
3. **Wikidata/FactGrid (Roscher-Lexikon als Linked Open Data)**: >15 000
   mythologische Subjekte, per SPARQL mehrsprachig abfragbar — die
   langfristig mächtigste, aber aufwendigste Quelle (eigener Backend-Typ
   `wikidata-sparql`).

Empfehlung: 1 zuerst (ein Nachmittag), 3 später, wenn der Nexus steht.

### N6 — Formfilter (S) und Begründungssatz (S)

- Silbenzähler: Vokalgruppen zählen, sprachspezifische Ausnahmen minimal —
  deterministisch, ~60 Zeilen. Flags `--syllables 2`, `--max-len 8`.
- Begründungssatz als deterministisches Template aus vorhandenen Feldern:
  `„spoor" — ndl./engl. ‚Fährte'; über related(track) ← Query ‚suchen';
  Register: Jägersprache.` Kein LLM nötig — alle Bausteine (Wurzel, Spur,
  Register) stehen dann im Datenmodell.

### E1 — Embeddings als Experimentierspur (M, hinter Benchmark-Gate)

Nicht die Hoffnung, aber ein Versuch wert — als **Zusatzsignal**, nie als
einzige Begründung (die Spur muss aus Graph/Glossen erklärbar bleiben):
- Kandidat 1: **ConceptNet Numberbatch** (mehrsprachig, passt zum Kantenpack,
  auf DB-Vokabular beschnitten + int8-quantisiert ≈ 10–20 MB Datei).
- Kandidat 2 (beobachten): statische Distill-Embeddings der neuen Generation
  (model2vec/„potion"-Klasse) — klein, schnell, ohne Laufzeit-Modell.
- Integration: optionales Pack via `db fetch`; Score-Anteil nur additiv;
  `benchmarks/` entscheidet, ob es bleibt (Leitplanke 5: raus, wenn es das
  Tool aufbläht, ohne die Benchmark-Fälle zu verbessern).

## Offen halten (Watch-List)

- Datamuse `topics=`-Parameter und `rel_syn/rel_spc/rel_gen` als weitere
  Online-Relationen.
- kaikki-URL-Deprecation (siehe N3).
- WordNet/OpenThesaurus als weitere Offline-Kantenquellen (en/de).
- Idiom-/Sprichwort-Bestände (Wiktionary-Kategorien) für Poesie.

## Reihenfolge & Definition of Done

1. **N1 + N2** (eine Runde): edges-Tabelle + Register — Assoziationen und
   Metaphern aus Daten, die wir schon haben.
2. **N3** (nebenbei): alte Sprachen in sources.yaml.
3. **N4** ConceptNet-Pack, dann **N5.1** Mythologie-JSON.
4. **N6** Formfilter + Begründungssatz.
5. **E1** parallel als Experiment, Benchmark entscheidet.

Done heißt: `spoor find "werkzeug das namen über bedeutung findet"` liefert
in den Top 5 ein Wort der Klasse *spoor/Fährte/vestigium* — mit sichtbarer
Kanten-Spur statt Glückstreffer. Der Benchmark (`docs/prompts.md`) bekommt
dafür eigene Assoziations-Fälle, sobald N1 gemergt ist.
