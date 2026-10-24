# Rauschanalyse (`analysis.noise`)

Menü: **Filter › Noise › Noise Analysis…**. Sie misst das Rauschen der aktiven Pixel-Ebene
(mit Auswahl: nur der Auswahl) und meldet Zahlen, die eine Rauschreduktion bemessen.

## Was gemessen wird

| Wert | Bedeutung |
|---|---|
| σ (8-bit) | Luminanz-Rauschen als Standardabweichung in 8-Bit-Stufen (0–255). Darunter R/G/B einzeln |
| Flat areas | Rauschen in den ruhigsten 10 % der 16×16-Blöcke — das Korn in Flächen, nicht an Kanten |
| Clipped shadows / highlights | Anteil der Messpunkte, die komplett auf Schwarz bzw. Weiß liegen |
| Samples | Zahl der tatsächlich gemessenen Pixel |

## Empfehlung direkt anwenden

**Filter › Noise › Reduce Noise from Analysis…** misst und öffnet sofort den Dialog
„Rauschen reduzieren" mit allen fünf empfohlenen Werten. Dort lässt sich die Vorschau prüfen, bevor
OK den Filter anwendet. Reine Zahlen ohne Dialog: **Filter › Noise › Noise Analysis…**.

## Empfehlung für „Rauschen reduzieren"

Die Ausgabe enthält unter `recommended` alle fünf Dialogwerte:

| Feld | Bereich | Ableitung |
|---|---|---|
| `strength` | 0–10 | 1,5 × Flat-σ (8-bit), gerundet; unter 0,4 → 0 |
| `preserveDetails` | 0–100 % | 60 + (3 − Flat-σ) × 10, begrenzt auf 20–90: feines Korn → mehr Detailerhalt |
| `reduceColorNoise` | 0–100 % | 40 × (Chroma-σ / Luma-σ), begrenzt auf 0–100: Farbstich-Rauschen des Scanners |
| `sharpenDetails` | 0–100 % | (Flat-σ − 1) × 8, begrenzt auf 0–60: Ausgleich für die Glättung |
| `removeJpegArtifact` | ja/nein | 8-Pixel-Blockkanten mindestens 15 % stärker als andere Kanten und Rauschen vorhanden |

Chroma-σ wird aus den Ebenen R−G und B−(R+G)/2 geschätzt, die Blockigkeit aus dem Verhältnis der
Horizontalgradienten auf dem 8-Pixel-Raster zu allen anderen (JPEG-Artefakte erzeugen dort Stufen).

Die Schätzung basiert auf dem Hochpass-Rest `4p − (links+rechts+oben+unten)`: bei weißem Rauschen
liefert der Mittelwert der Beträge die Standardabweichung `σ = √(π/2)/√20 · mean|rest|`. Gemessen
wird auf einem Raster von höchstens 1,5 Mio. Punkten, damit auch 100-MP-Scans sofort antworten.

## Interpretation für gescannte Negative

- **σ ≈ 1–2** — sauberer Scan, Korn dominiert; keine Rauschreduktion nötig.
- **σ ≈ 2–4** — sichtbares Korn/Scannerrauschen; Reduce Noise mit der vorgeschlagenen Stärke
  (etwa 15–35) und „Preserve Details“.
- **σ > 4** — starkes Rauschen (hoher ISO, Schatten aufgehellt); Stärke 40–70, Farbrauschen separat
  („Reduce Color Noise“). Bei Negativen zuerst die Inversion/Umkehr machen, dann messen — sonst
  misst man das Orange der Maskierung mit.
- **Hohe Clipped-Werte** — der Scan ist beschnitten (Schatten zugehalten oder Lichter ausgefressen);
  Rauschreduktion rettet das nicht, hier hilft ein erneuter Scan mit anderer Belichtung.
- Filmkorn ist **korreliert** (Körnungsgröße), Scannerrauschen meist nicht. Ein hoher Flat-Wert bei
  niedrigem Gesamt-σ deutet auf feines, gleichmäßiges Korn; ein hoher Gesamt-σ bei niedrigem Flat
  auf Kanten/DETAIL, das keine Reduktion braucht.

## Grenzen

- Misst in den gespeicherten (kodierten) Werten, nicht in linearem Licht. Für Vergleiche zwischen
  Bildern reicht das; absolute physikalische Werte sind es nicht.
- Die σ-Schätzung ist konservativ an stark strukturierten Bildern (Kanten zählen als Rauschen);
  deshalb der Flat-Wert als zweite Zahl.
- Die Analyse ändert das Dokument nicht.

## Automatisierung

```json
{"method":"engine.execute","params":{"command":"analysis.noise","params":{}}}
```

Antwort (gekürzt):

```json
{"luma8":2.8,"sigma8":[2.6,2.7,2.9],"flat8":2.1,"chroma8":1.4,"blockiness":1.02,
 "clippedShadows":0.002,"clippedHighlights":0.0,
 "recommended":{"strength":3,"preserveDetails":69,"reduceColorNoise":27,"sharpenDetails":9,"removeJpegArtifact":false},
 "samples":1499876,"layer":2,"bounds":[0,0,6000,4000]}
```

`layer` wählt eine andere Ebene; ohne Angabe gilt die aktive Ebene.
