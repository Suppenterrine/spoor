# Research: Der semantische Abstand zum North Star

Datum: 2026-07-09
Status (2026-07-10): Bausteine A (Fetch-Kuratierung), B (Konzeptbrücke),
C (Anti-Echo, Herkunfts-Bonus, Pfad in explain), D-1 (Datamuse rel_trg
Assoziations-Trigger) und E (lateinische Schrift: translit-Spalte aus
kaikki-Romanisierungen, regelbasierter Fallback, `[output] script`) sind
umgesetzt. Offen: D-2/D-3 (ConceptNet-Kanten als offline-Datenpack,
quantisierte Numberbatch-Embeddings) — erst angehen, wenn die Trefferquote
mit vollem Bestand (20k/Quelle) weiterhin enttäuscht.
Abweichung von der Analyse: Etymologie ist beim Fetch NICHT Pflicht (hätte
he/grc-Ausbeute zu stark beschnitten), nur eine brauchbare Glosse.
Fortsetzung: Der konkrete Plan für die verbleibende Lücke (Assoziationen,
Metaphern, Poesie, Formfilter, Begründung — ohne LLM) steht in
`docs/design/llm-parity.md`.

## Anlass

Die UX-Verbesserungen der letzten Commits (Status-Screen, Rich-TTY-Output,
Zero-Setup, `--online`) haben die Bedienung verbessert — aber der Kern-Use-Case
aus dem North Star („aus der Bedeutung eines Anwendungsfalls den passenden
Namen finden") bleibt hinter der Erwartung zurück:

- `spoor find "Baum"` liefert **Baum** — ein Echo, kein Treffer. Der North Star
  will Assoziation mit Abstand („größere, höhere Bedeutung"), nicht Identität.
- Getestete Anwendungsfälle liefern keine Namen, die man wirklich nehmen würde.
- Wörter aus el/grc/he kommen in nicht-lateinischer Schrift heraus — für ein
  CLI-Naming-Tool unbrauchbar als Standardausgabe.

## Der Maßstab: das LLM-Beispiel, das den Namen „spoor" fand

Der Projektname selbst stammt aus einer LLM-Abfrage. Sie ist der
Qualitätsmaßstab, an dem `find` gemessen wird. Originalverlauf:

> **Prompt:** Ich brauche eine besseren Namen für dieses Programm. Höchstens
> zweisilbig, deutsch, englisch, griechisch, Latein oder hebräisch. Der Name
> muss groß genug sein um den Zweck genau auszudrücken und Platz für die
> Bedeutung zu lassen. *(+ vollständiger North-Star-Text als Kontext)*
>
> **Antwort (gekürzt auf die Kandidaten):**
>
> - **AURA** (lat. *aura* – Hauch, Luftzug) — Der Moment, in dem ein Name
>   ankommt, ist ein Hauch – nichts Greifbares, aber spürbar.
> - **NOMEN** (lat. *nomen* – Name) — Direkt, selbstreferentiell. Ein Wort, das
>   seinen eigenen Zweck ausspricht (*nomen est omen*).
> - **KERN** (ahd. *kerno* – Kern, Samen) — Was du suchst, ist nicht der Name
>   selbst – es ist der Kern dessen, was der Name tragen soll.
> - **RUF** (ahd. *hruof* – Ruf, Schrei, Name) — Der älteste Name ist der, den
>   jemand ruft. Vereint Anrufung und Reputation.
> - **SPOOR** (engl./niederl. *spoor* – Fährte, Spur) — Du folgst der Spur der
>   Bedeutung bis zum Wort. Jägersprache – nicht zufällig, sondern
>   zielgerichtet. Eine Fährte hat immer einen Ursprung.
> - **FADE** (engl. *fade*) — Ein Name faded herein. Nicht plötzlich da,
>   sondern langsam klar.
> - **TONE** (griech. *tonos* – Spannung, Ton, Akzent) — Der richtige Name
>   trägt den richtigen Ton.
> - **LEXI** (griech. *lexis* – Wort, Rede) — Aus *lexis*, dem Wort selbst.
>
> **Empfehlung: SPOOR** — drückt genau aus, was das Tool tut: es folgt einer
> Fährte (der Bedeutung) bis zum Ziel (dem Namen). Nicht zufällig, sondern
> zielsuchend. Herkunft transparent.

### Was das LLM hier leistet (Zerlegung in Teilschritte)

1. **Konzeptextraktion** — aus der Beschreibung die tragenden Konzepte
   destillieren: *suchen, Bedeutung, Herkunft, Spur, ein Wort, ankommen*.
2. **Assoziative Expansion mit Abstand** — nicht Synonyme, sondern Metaphern:
   „Namen finden" → *Fährte lesen* (Jägersprache), → *Hauch* (Ankommen), →
   *Kern* (Essenz). Genau der Sprung, den `find` heute nicht macht.
3. **Cross-Sprach-Projektion** — jedes Konzept in la/grc/ahd/he/en/nl-Lexeme
   übersetzen (*aura*, *tonos*, *hruof*, *spoor*).
4. **Formfilter** — max. 2 Silben, aussprechbar, lateinische Schrift.
5. **Begründung aus Etymologie** — jede Empfehlung nennt Wurzel und erklärt
   den Bedeutungsbogen zurück zur Aufgabe.

Schritte 1, 3, 4, 5 sind **deterministisch reproduzierbar** mit Daten, die wir
bereits fetchen (kaikki-Glossen + Etymologien). Nur Schritt 2 — der
Metaphernsprung — braucht eine echte Assoziationsquelle (Graph oder
Embeddings). Das ist die eigentliche Lücke.

## Warum `find` heute scheitert — fünf konkrete Befunde

Stand: `src/lookup/mod.rs`, `src/fetch/mod.rs`, `sources.yaml`, `data/words.db`
mit ~15 400 Wörtern (77 kuratiert + je ~2 000–2 800 pro kaikki-Quelle).

### 1. Echo statt Assoziation (Scoring-Design)

`score_record` gibt für exakten Worttreffer die höchste Punktzahl (5.0). Die
Query „Baum" rankt den Datensatz „Baum" zwangsläufig auf Platz 1. Der North
Star fordert das Gegenteil: Identität ist der uninteressanteste Treffer.

### 2. Der Wortbestand ist willkürlich (Fetch nimmt Dump-Reihenfolge)

`db fetch` akzeptiert die **ersten N Zeilen** jedes kaikki-Dumps. Die Dumps
sind nach Wiktionary-Seitenerstellung geordnet — die ersten englischen Wörter
in der DB sind: *dictionary, free, thesaurus, encyclopedia, portmanteau,
encyclopaedia, cat, gratis, word, livre, …* Latein beginnt mit *thesaurus,
encyclopaedia, pie, aquila, December, September, …*

Folge: Das Vokabular ist kein kuratierter Namensraum, sondern ein Zufallsauschnitt.
Pointiert: **„Baum" existiert in der DB nur als englischer Nachname**
(Gloss: „a surname, a german jewish surname") — nicht als deutsches Wort für
Baum, nicht als Konzept *tree*.

### 3. Rein lexikalisches Matching, keine Sprachbrücke

Gematcht wird Token gegen `word`, `tags` (= 2 gekürzte englische Glossen),
`system`, `etymology` — per Exakt/Präfix/Substring. Eine deutsche Query trifft
englische Glossen nie; *arbor* (la) wird für „Baum" nie gefunden, obwohl seine
Glosse „tree" enthält. Es gibt keinerlei Synonymie, Übersetzung oder
Assoziation im Ranking selbst.

### 4. `--online` (Datamuse) greift fast nie

`expand_query` holt `ml=`-Kandidaten (englische Synonyme) und die zählen nur,
wenn sie **exakt** einem DB-Wort entsprechen. Bei ~2 000 willkürlichen
englischen Wörtern ist die Trefferwahrscheinlichkeit nahe null. Zudem:
Datamuse ist englisch-only (deutsche Queries → nichts) und `ml` liefert
Bedeutungsnähe, nicht den gewünschten Assoziationsabstand (`rel_trg`/`topics`
wären dafür besser).

### 5. Nicht-lateinische Schrift in der Ausgabe

el/grc/he-Wörter werden in Originalschrift gespeichert und ausgegeben
(σοφία, חכמה). Kaikki liefert Romanisierungen im `forms`-Feld (Tag
`romanization`), die wir beim Parsen derzeit wegwerfen.

## Lösungsraum

Rahmenbedingungen aus den Leitplanken: deterministisch (3), Binary klein —
Modelle/Daten als optionale Packs, nicht einkompiliert (4, 5), Herkunft
sichtbar (2), Standardausgabe ein Wort (1).

### Baustein A — Wortbestand kuratieren statt Dump-Reihenfolge

Beim Fetch filtern statt blind die ersten N nehmen: Einträge ohne Glosse oder
ohne Etymologie überspringen; Glossen mit *surname, initialism, abbreviation,
misspelling, obsolete, alternative form/spelling of* verwerfen; `max_words`
deutlich erhöhen (z. B. 20 000/Quelle — SQLite verkraftet das mühelos, der
Early-Abort-Mechanismus bleibt). Optional: Frequenzliste als Qualitätsfilter.
**Aufwand: klein. Wirkung: groß — ohne guten Kandidatenpool hilft kein Ranking.**

### Baustein B — Konzeptbrücke über die englischen Glossen (der Schlüssel)

Alle kaikki-Quellen (de/la/el/grc/he) stammen aus dem englischen Wiktionary →
**alle Glossen sind Englisch**. Das ist eine bereits vorhandene,
deterministische Interlingua:

```
Query "Baum" (de)
  → DB-Eintrag de/Baum, Glosse "tree"        (de→en-Übersetzung: gratis!)
  → Konzept-Token "tree"
  → alle Wörter beliebiger Sprache mit "tree" in der Glosse:
     arbor (la), δέντρο (el), δένδρον (grc), עץ (he), Hain (de), …
```

Umsetzung: invertierter Index Glossen-Token → Wort-IDs (SQLite FTS5 oder
eigene Tabelle), Glossen-Token IDF-gewichtet (Füllwörter wie *of, person,
plural* dürfen fast nichts zählen). Zweistufige Suche: Query-Tokens → eigene
Glossen als Konzepte → Konzepte gegen den Index. Offline, deterministisch,
kein neues Binary-Gewicht. **Das ist die wichtigste Einzelmaßnahme.**

### Baustein C — Anti-Echo und Herkunfts-Bonus im Scoring

- Identität bestrafen: `record.word == query_token` (case-/translit-insensitiv)
  → Score stark dämpfen oder ausschließen (Flag `--allow-echo` für den alten
  Modus).
- Abstand belohnen: Treffer über die Konzeptbrücke (Hop 1–2) höher gewichten
  als lexikalische Treffer; Ursprungs-/Fremdsprachen (la, grc, he, ahd, non)
  bekommen einen Bonus gegenüber der Query-Sprache — das erzeugt die „höhere,
  entferntere Bedeutung".
- Formfilter als Flags: `--syllables 2`, `--max-len`, Aussprechbarkeit.
- `explain` zeigt den Assoziationspfad — die *Spur*:
  `arbor — lat. 'Baum' · Pfad: Baum → tree → arbor`.

### Baustein D — Assoziationsabstand (der Metaphernsprung)

Für den Sprung *„Namen finden" → „Fährte"* reicht die Glossenbrücke nicht.
Optionen, aufsteigend nach Aufwand:

1. **Datamuse besser nutzen (online):** statt nur `ml` auch `rel_trg`
   (Assoziations-Trigger) und `topics=`; Kandidaten nicht nur gegen `word`,
   sondern gegen die Glossen/Konzeptbrücke matchen — dann wirkt die Expansion
   auch auf la/grc/he-Wörter. Bleibt englisch-only und online-only.
2. **ConceptNet-Kanten als optionales Datenpack (offline):** relevante
   Relationen (RelatedTo, SymbolOf, UsedFor, HasA) für unser Vokabular beim
   Fetch extrahieren → kleine Kantentabelle in SQLite. Deterministisch,
   mehrsprachig, erklärbar („RelatedTo: track → hunt").
3. **Statische multilinguale Embeddings als optionales Pack:** ConceptNet
   Numberbatch (mehrsprachig, deterministisch), beim Fetch auf DB-Vokabular +
   häufige Query-Wörter beschnitten, 8-bit-quantisiert → ~10–20 MB Datei
   neben `words.db`, nie im Binary. Cosine-Ranking, reproduzierbar. Stärkster
   semantischer Effekt, aber Begründung („warum passt das?") wird schwächer —
   müsste mit B kombiniert werden, damit die Spur sichtbar bleibt.

Empfohlene Reihenfolge: erst 1 (billig), dann 2; 3 nur, falls die Trefferquote
danach immer noch enttäuscht (Leitplanke 5: Tool vor Technik).

### Baustein E — Lateinische Schrift als Standard

- Beim Fetch Romanisierung aus kaikki `forms[].tags == ["romanization"]`
  extrahieren → neue Spalte `translit`.
- Fallback: regelbasierte Transliterationstabellen für el/grc/he (klein,
  deterministisch, ~50 Zeilen pro Schrift).
- Ausgabe standardmäßig lateinisch: `sophia (σοφία)`; Config
  `[output] script = "latin" | "native" | "both"`.

## Zielbild nach Umbau (Soll-Beispiel)

```
$ spoor find "CLI, die Logs von verteilten Systemen synchronisiert"
cohaerere — lat. 'zusammenhängen' · Pfad: synchronisieren → cohere → cohaerere
```

Ein Wort, lateinische Schrift, Herkunft und Spur sichtbar, deterministisch —
`--count 5` für die Handvoll, `--seed` für reproduzierbare Alternativläufe.

## Bewusst nicht verfolgt

- **LLM im Backend:** widerspricht Determinismus (Leitplanke 3) und
  Binary-Disziplin (5). Der Maßstab bleibt das LLM-Beispiel, das Mittel nicht.
- **Embeddings im Binary:** nur als optionales Datenpack (Leitplanke 4/5).
