# Auto Correct (`image.autoCorrect`)

Menü: **Image › Auto Correct…** (unter Auto Tone/Auto Contrast/Auto Color).

Die Analyse ersetzt „nach Gefühl drehen": PhotoCraft misst das Histogramm der aktiven Pixel-Ebene
(oder der Auswahl) und legt **zwei Einstellungsebenen** darüber — nichts wird in die Pixel
geschrieben, alles bleibt nachträglich änderbar.

| Wert | Herkunft |
|---|---|
| Schwarzpunkt (`inBlack`) | 0,1 %-Perzentil der kombinierten Helligkeit, skaliert mit der Stärke |
| Weißpunkt (`inWhite`) | 99,9 %-Perzentil, skaliert mit der Stärke |
| Gamma R/G/B | gleicht die Kanal-Mediane an (Grauwelt-Weißabgleich) und erhält den Farbcharakter |
| Dynamik (`vibrance`) | nur bei stumpfen, aber farbigen Bildern (mittlere Sättigung 5–35 %); Grau bleibt grau |

Ergebnis:

- `Auto Correct – Levels` (Tonwertkorrektur + Kanal-Gamma)
- `Auto Correct – Vibrance` (nur wenn die Sättigung es hergibt)

Beide Ebenen lassen sich im Ebenen-Panel ein-/ausblenden, mit Deckkraft und Masken versehen oder
über ihre Einstellungen nachjustieren. Die Meldung nennt die verwendeten Werte; die JSON-Antwort
enthält `black`, `white`, `gamma`, `saturation`, `vibrance` und die Ebenen-IDs.

## Stärke

`{"strength": 0..100}` (Standard 100) blendet jeden berechneten Wert gegen „keine Änderung":
0 lässt Schwarz-/Weißpunkt bei 0/255, alle Gammas bei 1 und die Dynamik bei 0. Praktisch für
einen sanften Look oder für Serien, bei denen nur ein Teil der Korrektur gewünscht ist.

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
{"applied":true,"black":12,"white":243,"gamma":[1.0,1.03,0.98],
 "saturation":0.28,"vibrance":6,"levelsLayer":7,"vibranceLayer":8}
```

## Grenzen

- Misst die aktive Ebene; Einstellungsebenen wirken auf alles darunter (Photoshop-Verhalten),
  also bei Bedarf die Ziel-Ebene vorher nach oben holen.
- Ein stark beschnittenes Bild (Schatten zu, Lichter ausgefressen) kann nicht zurückgewonnen
  werden; Auto Correct spreizt nur den vorhandenen Tonwertumfang.
- Für gescannte Negative vor dem Korrigieren invertieren, sonst wird die Orange-Maskierung
  mitkorrigiert.
