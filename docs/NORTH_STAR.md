# North Star — spoor

## Der eine Satz

Ein Werkzeug, das aus der Bedeutung eines Anwendungsfalls den passenden Namen findet — als einzelnes Wort, mit einer Spur, die bis zu seiner Herkunft zurückverfolgbar ist.

## Wofür wir das bauen

Weil Namen oft das Härteste sind.  
Nicht weil einem nichts einfällt, sondern weil das, was einfach *passt*, selten ist.  
Weil Zufallsgeneratoren Wörter produzieren, aber keine *Treffer*.  
Und weil ein guter Name mehr ist als ein Klang: er trägt eine Bedeutung.

Dieses Tool ist kein Generator zum Generieren.  
Es ist ein **Suchgerät für Begriffe**, das Zufall und Semantik kombiniert.

## Was es am Ende tut

Jemand beschreibt einen Anwendungsfall:  
> *„Eine CLI, die Logs von verteilten Systemen synchronisiert.“*

Das Tool gibt ein einzelnes Wort zurück — oder eine handvoll — mit Begründung:  
> **„Kohärenz“ — aus lat. *cohaerere* (zusammenhängen), Bezug zu Synchronisation, Logik, Verlässlichkeit.**  
> **„Faden“ — metaphorisch: roter Faden durch verteilte Datenströme.**  
> **„Riss“ — negativ assoziiert, aber präzise für Fehlerfälle.**

Man wählt. Oder man generiert noch einen Lauf mit anderem Seed.

## Leitplanken

1. **Ein Wort vor fünf.**
   Die Standardausgabe des Lookup ist ein einzelner Begriff, nicht eine Liste. Listen sind Mittel, kein Ziel.

2. **Herkunft sichtbar.**
   Jedes Ergebnis nennt seine Wurzel. Keine Blackbox-Wörter. Der Nutzer soll verstehen, *warum* es passt.

3. **Seed über Persistenz.**
   Wer einmal einen guten Seed findet, behält ihn. Reproduzierbarkeit ist Bedingung, kein Extra.

4. **Datenbank wächst, Zwang nicht.**
   Basiswörter sind sofort da. Curated Systems (Mythologie, Natur, Handwerk) fallen nach Bedarf hinzu. Niemand muss alles installieren, um ein Wort zu generieren.

5. **Tool vor Technik.**
   Binary zuerst. Wenn ein Modell oder ein Algorithmus das Binary verlangsamt oder aufbläst → raus damit, bis es kleiner wird.

## Der echte Test

Wenn ich das Tool öffne, einen Anwendungsfall eintippe, und innerhalb von 10 Sekunden ein Wort habe, das ich *wirklich* benennen möchte — dann funktioniert es.  
Wenn ich danach noch 20 ähnliche Wörter bekomme, von denen keins besser ist — dann nicht.
