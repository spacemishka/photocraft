# Filmkorn extrahieren und wiederverwenden (`image.extractGrain`)

Menü: **Filter › Noise › Extract Grain…** (deutsch: „Korn extrahieren…").

PhotoCraft trennt das Korn einer Aufnahme vom Bildinhalt und legt es als **eigene Ebene** ab:

- Grundlage ist der Hochfrequenz-Rest `Pixel − lokaler Weichzeichner`.
- Zwei Stufen halten Bildstruktur heraus:
  1. **Bandpass:** Vom Rest wird nochmals ein Weichzeichner (doppelter Radius) abgezogen — was
     breiter ist als das Korn (Kanten, Verläufe, Objektformen), verschwindet.
  2. **Kantenmaske:** Wo der Weichzeichner Gradienten zeigt, wird die Umgebung um den dreifachen
     Radius neutral (0,5) gesetzt. Objektkanten tauchen dadurch nicht als helle/dunkle Säume auf.
- Das Feld ist auf 0,5 zentriert (neutral) und wird mit **Overlay** über das Bild gelegt.
- `radius` (1–20, Standard 4) bestimmt, welche Körnung als „fein" gilt: kleiner Radius = feineres
  Korn; größer = gröbere Strukturen.
- `monochrome` (Standard an) macht ein einfarbiges Korn; aus ergibt es Farbkorn pro Kanal.
- `strength` (0–200 %, Standard 100) skaliert die Amplitude; 100 % entspricht ±0,5 bei 3σ.

Ergebnis: eine Ebene `Film Grain` (bzw. `Film Grain 2`, …) mit Overlay. Die Meldung/JSON nennt
`sigma8` (Kornstärke in 8-Bit-Stufen, wie in der Rauschanalyse), `scale` und die Grenzen.

## Wiederverwenden

Eine Korn-Ebene ist eine normale Pixel-Ebene; es gibt keinen Sondercontainer:

1. **In ein anderes Bild kopieren:** Korn-Ebene markieren, Ctrl+C, im Zieldokument Ctrl+V —
   Overlay, Deckkraft und Inhalt kommen mit. Alternativ per Drag zwischen Tabs.
2. **Fein dosieren:** Deckkraft der Ebene (z. B. 30–70 %) oder eine Ebenenmaske.
3. **Skalieren:** Transformieren (Ctrl+T) vergrößert/verkleinert das Korn; für gleichmäßiges Korn
   vorher „Bearbeiten › Muster festlegen" bzw. als eigenes Dokument speichern.
4. **Dauerhaft aufbewahren:** Dokument mit nur dieser Ebene als `.pcraft` speichern (oder die
   Ebene exportieren) — beim nächsten Mal öffnen und kopieren.
5. **Auf eine Auswahl begrenzen:** Auswahl vor dem Kopieren/Einfügen verwenden; das Korn landet
   dann nur dort (Photoshop maskiert beim Einfügen nicht automatisch — ggf. Maske nutzen).

## Grenzen

- Sehr feines Korn (unter ~1/255) ist nach dem 8-Bit-Speichern nicht mehr vollständig da; aus
  16-/32-Bit-Quellen kommt mehr heraus.
- Feine Texturen (Laub, Stoff, Fell, Rauschen im Himmel) zählen teilweise als Korn; ein größerer
  `radius` trennt sie besser ab, kostet aber feines Korn. Für ein reines Korn-Sample eine ruhige
  Fläche (Himmel, Wand) auswählen und dort extrahieren.
- Kanten und Objektformen werden um den dreifachen Radius neutralisiert — dort ist die
  Kornebene flach. Beim Kacheln fällt das nicht auf, beim Übereinanderlegen mit dem Original
  ebenfalls nicht, weil 0,5 in Overlay unsichtbar ist.
- Starke JPEG-Artefakte landen teilweise im Korn; erst „Reduce Noise" mit „Remove JPEG Artifact",
  dann extrahieren.
- Bei gescannten Negativen zuerst invertieren, sonst wird die Orange-Maskierung mit extrahiert.
- Das Korn ist ortsfest: beim Verschieben der Ebene wandert das Muster mit (kein Kachel-Muster).
  Für kachelbare Wiederholung die Ebene als Muster definieren und eine Musterfüllung mit Overlay
  anlegen.

## Automatisierung

```json
{"method":"engine.execute","params":{"command":"image.extractGrain","params":{"radius":4,"monochrome":true,"strength":100}}}
```

Antwort (gekürzt):

```json
{"layer":8,"sigma8":3.4,"scale":0.14,"bounds":[0,0,6000,4000]}
```
