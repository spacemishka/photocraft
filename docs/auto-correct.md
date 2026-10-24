# Auto Correct (`image.autoCorrect`)

Menü: **Image › Auto Correct…** (unter Auto Tone/Auto Contrast/Auto Color).

Die Analyse ersetzt „nach Gefühl drehen": PhotoCraft misst das Histogramm der aktiven Pixel-Ebene
(oder der Auswahl) und legt **Einstellungsebenen** darüber — nichts wird in die Pixel
geschrieben, alles bleibt nachträglich änderbar.

| Ebene | Inhalt |
|---|---|
| `Levels 1` | ein normaler Levels-Kanal, in dem **pro Farbkanal** der leere Rand links und rechts abgeschnitten wird (0,1 %-Perzentile); der Master bleibt unangetastet |
| `Vibrance 1` | nur bei farbigen Bildern unter 55 % mittlerer Sättigung; Grau bleibt grau |

Beispiel: R 12–241, G 10–238, B 15–235 — jeder Kanal verliert nur die unbenutzten Enden, ein
Farbstich verschwindet dadurch ohne Mitteltönung. Flache Kanäle (Spanne < 2 %) werden nicht
gespreizt, sonst würde ein einziger Ton zu Schwarzweiß.

Beide Ebenen lassen sich im Ebenen-Panel ein-/ausblenden, mit Deckkraft und Masken versehen oder
über ihre Einstellungen nachjustieren. Die Meldung nennt die Kanalgrenzen; die JSON-Antwort
enthält `channels` (je `[schwarz, weiß]`), `saturation`, `vibrance` und die Ebenen-IDs.

## Stärke

`{"strength": 0..100}` (Standard 100) blendet jeden berechneten Wert gegen „keine Änderung":
0 lässt alle Kanalgrenzen bei 0/255 und die Dynamik bei 0. Praktisch für einen sanften Look oder
für Serien, bei denen nur ein Teil der Korrektur gewünscht ist.

## Abgrenzung zur KI

Die AI-Backends (stable-diffusion.cpp, ComfyUI) sind Bildgeneratoren und liefern keine
Bildanalyse. Auto Correct ist deshalb eine **messbasierte** Korrektur (wie Auto Tone/Color in
Photoshop), keine LLM-Empfehlung. Ein LLM-Berater wäre ein eigener Text-Backend-Modus; die
Statistik hier ist deterministisch, schnell und ohne Server verfügbar.

## Automatisierung

```json
{"method":"engine.execute","params":{"command":"image.autoCorrect","params":{"strength":100}}}
```

Antwort (gekürzt):

```json
{"applied":true,"channels":[[12,243],[10,238],[15,235]],
 "saturation":0.28,"vibrance":15,"levelsLayer":7,"vibranceLayer":8}
```

## Grenzen

- Misst die aktive Ebene; Einstellungsebenen wirken auf alles darunter (Photoshop-Verhalten),
  also bei Bedarf die Ziel-Ebene vorher nach oben holen.
- Ein stark beschnittenes Bild (Schatten zu, Lichter ausgefressen) kann nicht zurückgewonnen
  werden; Auto Correct spreizt nur den vorhandenen Tonwertumfang.
- Für gescannte Negative vor dem Korrigieren invertieren, sonst wird die Orange-Maskierung
  mitkorrigiert.
