# AI-Retusche in PhotoCraft – Benutzerhandbuch

Dieses Handbuch beschreibt die beiden KI-Befehle **Remove Object** (Objekt entfernen / generatives
Füllen) und **Skin Enhance** (Haut glätten), die lokalen Backends (stable-diffusion.cpp, ComfyUI,
Ollama-kompatibler Server), die Einrichtung und die Fehlerbehebung.

Stand: 24.10.2026, PhotoCraft 0.15.1, Branch `ai-inline-editing`.

---

## 1. Was die Funktionen tun

| Befehl | Menü | Aufgabe | Ergebnis |
|---|---|---|---|
| `ai.removeObject` | Edit › Content-Aware › Remove Object… | füllt eine Auswahl mit modelliertem Inhalt (Objekt entfernen, Fläche ergänzen) | neues Layer „AI: Remove Object 1“ (Standard) oder direkt im Layer |
| `ai.skinEnhance` | Filter › Neuronale Filter › Skin Enhance… | glättet Haut mit Frequenztrennung, Details bleiben erhalten | direkt im aktiven Layer |
| `ai.pingBackend` | Eigenschaften-Panel › AI Retouching › Test Connection | prüft das konfigurierte Backend | Statuszeile + Zeile unter dem Knopf |

Beide Befehle laufen als **Hintergrund-Job**:

- Fortschritt („Encoding“, „Refining“, „Detecting skin“, „Smoothing“) und **Cancel** auf der
  Leinwand-Karte; **Esc** bricht ab.
- Ein Abbruch lässt das Dokument **unverändert**.
- Jeder Befehl ist ein Schritt in der History (Rückgängig mit Ctrl+Z).
- Ohne erreichbares Backend fällt Remove Object auf den klassischen Content-Aware-/PatchMatch-
  Algorithmus zurück; Skin Enhance nutzt dann die HSV-Hautmaske.

---

## 2. Voraussetzungen

- **Pixel-Ebene** aktiv (Hintergrund oder normale Bildebene). Bei Text-, Form-, Gruppen-, Füll- und
  Smart-Object-Ebenen gibt es keine Pixel; der Befehl meldet „layer has no pixel surface“. Vorher
  rastern (`Layer › Rasterize`) oder eine andere Ebene wählen.
- Für Remove Object: **aktive Auswahl** (Rechteck, Lasso, Zauberstab, Objektauswahl …). Die Auswahl
  überlebt den Befehl.
- Größe der Auswahl ≤ `maxRegionMpx` (Standard 24 MP).
- Das Eigenschaften-Panel (Fenster › Eigenschaften) ist der Ort für die Bedienung; es zeigt den
  Abschnitt „AI Retouching“ **nur bei einer Pixel-Ebene**.

---

## 3. Backends und ihre Reihenfolge

`make_backend` wählt in dieser Reihenfolge:

| Priorität | Bedingung | Backend |
|---|---|---|
| 1 | `sd-cli Path` gesetzt | stable-diffusion.cpp (`sd-cli.exe`), ein Prozess pro Auftrag |
| 2 | `ComfyUI Workflow (API JSON)` gesetzt | ComfyUI über HTTP |
| 3 | sonst | Ollama-kompatibler Server unter `Local URL` |

Alle Einstellungen liegen unter **Edit › Preferences › KI…**. Die Zeile im AI-Retouching-Abschnitt
zeigt das aktive Backend (z. B. `sd-cli · F:\AI\stable-diffusion-cpp\sd-cli.exe`).

---

## 4. Einrichtung A: stable-diffusion.cpp (`sd-cli`)

### 4.1 Modelle herunterladen (FLUX.1 Fill, 16 GB VRAM)

| Zweck | Hugging-Face-Repo | Datei | Größe |
|---|---|---|---|
| Diffusion | `second-state/FLUX.1-Fill-dev-GGUF` | `flux1-fill-dev-Q4_0.gguf` | 6,93 GB |
| VAE | `second-state/FLUX.1-Fill-dev-GGUF` | `ae.safetensors` | 335 MB |
| CLIP-L | `comfyanonymous/flux_text_encoders` | `clip_l.safetensors` | 246 MB |
| T5-XXL | `city96/t5-v1_1-xxl-encoder-gguf` | `t5-v1_1-xxl-encoder-Q4_K_M.gguf` | ~3,1 GB |

Summe ≈ 10,6 GB; mit Rechenpuffern passt das in 16 GB. Bei Engpass: T5 als `Q3_K_M` (≈2,4 GB) oder
das Diffusionsmodell als `Q3_K` (5,4 GB).

```powershell
pip install -U "huggingface_hub[cli]"
mkdir F:\AI\models\flux-fill
hf download second-state/FLUX.1-Fill-dev-GGUF flux1-fill-dev-Q4_0.gguf ae.safetensors --local-dir F:\AI\models\flux-fill
hf download comfyanonymous/flux_text_encoders clip_l.safetensors --local-dir F:\AI\models\flux-fill
hf download city96/t5-v1_1-xxl-encoder-gguf t5-v1_1-xxl-encoder-Q4_K_M.gguf --local-dir F:\AI\models\flux-fill
```

Lizenz: FLUX.1 **Fill dev** steht unter der *FLUX.1 [dev] Non-Commercial License* — privat/testweise
frei, kommerziell nicht.

### 4.2 Einstellungen

| Feld | Wert (Beispiel) |
|---|---|
| `sd-cli Path` | `F:\AI\stable-diffusion-cpp\sd-cli.exe` |
| `Diffusion Model` | `F:\AI\models\flux-fill\flux1-fill-dev-Q4_0.gguf` |
| `VAE` | `F:\AI\models\flux-fill\ae.safetensors` |
| `CLIP-L` | `F:\AI\models\flux-fill\clip_l.safetensors` |
| `T5-XXL` | `F:\AI\models\flux-fill\t5-v1_1-xxl-encoder-Q4_K_M.gguf` |
| `Extra sd-cli Arguments` | `--steps 20 --guidance 30 --cfg-scale 1 --sampling-method euler --vae-tiling` |
| `Local timeout secs` | `300` (erster Lauf lädt das Modell) |

Für SDXL-/SD1.5-Inpainting-Modelle stattdessen z. B.
`--steps 25 --cfg-scale 7 --sampling-method euler --vae-tiling` (kein `--guidance`) und `--clip_l`
leer lassen; `--diffusion-model` zeigt dann auf das Inpainting-Modell.

### 4.3 Was PhotoCraft aufruft

```text
sd-cli.exe --diffusion-model <Modell> --vae <VAE> [--clip_l <CLIP-L>] [--t5xxl <T5>] \
  -i <temp>\source.png --mask <temp>\mask.png -o <temp>\out.png \
  -W <Breite> -H <Höhe> -p "<Prompt>" [ -n "<Negativprompt>" ] [ -s <Seed> ] \
  --strength <Stärke> <Extra-Argumente>
```

- Auswahl wird auf ein Vielfaches von 32 aufgerundet, das Ergebnis zurück auf die Auswahl skaliert.
- Weiß in der Maske = neu malen (PhotoCraft-Auswahl).
- Nach `Local timeout secs` wird der Prozess beendet und ein Fehler gemeldet.
- Fehlendes `sd-cli.exe` oder leerer Modellpfad: Konfigurationsfehler (kein stiller Fallback).
- Der erste Lauf enthält das Laden der Gewichte (bei FLUX Q4 ca. 20–60 s); danach ist jeder Auftrag
  ein kompletter Neustart des Prozesses (erneutes Laden).

---

## 5. Einrichtung B: ComfyUI

1. ComfyUI starten, FLUX.1 Fill als Workflow bauen (oder ein vorhandenes Beispiel laden).
2. **Workflow › Export (API)** — die *API-JSON*, nicht die normale Workflow-Datei.
3. Datei speichern, Pfad in `ComfyUI Workflow (API JSON)` eintragen, `ComfyUI URL` prüfen
   (Standard `http://127.0.0.1:8188`).
4. Platzhalter in die Feldwerte schreiben (der Backend ersetzt sie vor dem Queueing):

| Platzhalter | Bedeutung | Typische Nodes |
|---|---|---|
| `{{image}}` | hochgeladenes Quellbild | `LoadImage.image` |
| `{{mask}}` | hochgeladene Maske (weiß = neu malen) | `LoadImageMask` oder `LoadImage` + `ImageToMask` |
| `{{prompt}}` | Prompt des Befehls | `CLIPTextEncode` (positiv) |
| `{{negative_prompt}}` | Negativprompt | `CLIPTextEncode` (negativ) |
| `{{seed}}` | Seed als Zahl | `KSampler.seed` |
| `{{strength}}` | 0..1 als Zahl | `KSampler.denoise` |
| `{{width}}`, `{{height}}` | Auswahlgröße als Zahlen | `EmptyLatentImage` |
| `{{task}}` | `inpaint` | optional für Verzweigungen |

Ein Wert, der **genau** ein numerischer Platzhalter ist (`"{{seed}}"`), wird als JSON-Zahl
eingesetzt — so funktionieren `KSampler` und `EmptyLatentImage` direkt. Platzhalter in längeren
Texten werden als Text ersetzt (z. B. `"a photo of {{prompt}}"`).

Hinweise:

- Das Backend nimmt das **erste Bild vom Typ `output`** aus den Node-Ausgaben; `SaveImage` (nicht
  nur „Preview Image“) muss im Workflow hängen.
- Bei echten FLUX-Fill-Workflows sind Bild und Maske über `InpaintModelConditioning` verbunden;
  die Maske im `LoadImageMask`-Node sollte auf Weiß = reparieren stehen.
- ComfyUI braucht für GGUF-Modelle das Custom-Node **ComfyUI-GGUF**.
- Modelle nach `ComfyUI\models\`: `diffusion_models` (oder `unet`) für die GGUF, `vae` für
  `ae.safetensors`, `text_encoders` für `clip_l.safetensors` und die T5-GGUF.

---

## 6. Einrichtung C: Ollama-kompatibler Server

Nur relevant, wenn ein Server die PhotoCraft-Erweiterung spricht: `POST {url}/api/generate` mit
`model`, `task`, `images: [base64-PNG]`, `mask`, `strength`, `prompt` und einer Antwort
`{"response": "<base64-PNG>"}`; `GET {url}/api/tags` für den Test. Ein normales Chat-/Vision-Modell
(llava & Co.) liefert Text und keine Pixel — dafür ist der Bild-Server nötig. Modelle wie FLUX
gehören nicht in llama.cpp, sondern in stable-diffusion.cpp (Abschnitt 4) oder ComfyUI (5).

---

## 7. Bedienung: Objekt entfernen

1. Pixel-Ebene wählen, Auswahl um das Objekt legen (etwas Rand lassen).
2. **Eigenschaften › AI Retouching**:
   - **Test Connection** — zeigt `sd-cli: AI backend is reachable` (grün) oder den Fehler (rot).
   - **Remove Object**: Prompt (optional), Strength (Standard 85 %), „Output to a new layer“.
   - **Replace with surrounding background (no AI)** — statt des Modells füllt PhotoCraft die
     Auswahl aus den umliegenden Pixeln (klassisches Content-Aware/PatchMatch). Schnell, ohne
     Modell, ohne Prompt; ideal für gleichmäßige Flächen (Himmel, Wand, Wiese). Der Prompt wird
     dabei ignoriert und ausgeblendet.
3. **Remove Object** klicken.

**Prompt-Semantik (wichtig für FLUX Fill):** beschreibe, **was dort entstehen soll**, nicht was
verschwindet. Beispiel: Person auf einer Wand → `"bare wall, matching plaster texture"` statt
„Person entfernen“. Leer lassen heißt „fülle passend zur Umgebung“.

Ablauf und Rückmeldung:

- Fortschrittskarte in der Bildmitte: Titel, Status, Prozent, Laufzeit, **Cancel**; Esc bricht ab.
- Bei Erfolg entsteht `AI: Remove Object 1` über der Quelle und wird aktiv; die Auswahl bleibt.
- Fehler (Modell fehlt, Timeout, Serverfehler) erscheinen in der Statuszeile und im Job-Dialog; das
  Dokument bleibt unverändert.
- Bei einem **Netzwerkfehler** (Server nicht erreichbar) läuft der klassische PatchMatch-Inpaint. Ein
  fehlender Pfad oder eine leere Modellangabe ist dagegen ein Konfigurationsfehler und wird gemeldet.

**Strength:** 0,85 ist ein guter Start. Höher = stärker neu erfunden (bei FLUX fast egal, die Maske
zählt); niedriger = näher am Original.

---

## 8. Bedienung: Haut glätten

1. Pixel-Ebene mit Personen wählen (keine Auswahl nötig; der Befehl bearbeitet die ganze Ebene).
2. **AI Retouching › Skin Enhance**: Strength (0–100), Radius (2–20 px), „Preserve Detail“.
3. **Apply Skin Enhance**.

- Mit einem Segmentierungs-Backend würde die Hautmaske vom Modell kommen; die aktuellen lokalen
  Backends (sd-cli/ComfyUI) unterstützen das **nicht**, deshalb nutzt PhotoCraft die HSV-Hautmaske
  (Hue 330–50°, Sättigung 15–90 %, Helligkeit 20–95 %). Das Ergebnis ist klassische
  Frequenztrennung, kein neuronales Retuschieren.
- Wirkung nur auf hautfarbene Pixel; der Rest bleibt unangetastet.

---

## 9. Einstellungen im Überblick (Edit › Preferences › KI…)

| Feld | Standard | Bedeutung |
|---|---|---|
| `Local url` | `http://127.0.0.1:11434` | Ollama-kompatibler Server (nur ohne sd-cli/ComfyUI) |
| `Local model` | `llava` | Modellname für diesen Server |
| `Local timeout secs` | `120` | Timeout für Anfrage **und** sd-cli-Prozess (für FLUX: 300) |
| `Max region mpx` | `24` | Obergrenze der Auswahlfläche |
| `ComfyUI URL` | `http://127.0.0.1:8188` | ComfyUI-Adresse |
| `ComfyUI Workflow (API JSON)` | leer | Pfad zur API-Workflow-Datei; leer = aus |
| `sd-cli Path` | leer | Pfad zu `sd-cli.exe`; gesetzt = sd-cli-Backend |
| `Diffusion Model` | leer | `--diffusion-model` |
| `VAE` | leer | `--vae` |
| `CLIP-L` | leer | `--clip_l` (darf leer sein) |
| `T5-XXL` | leer | `--t5xxl` (FLUX braucht es) |
| `Extra sd-cli Arguments` | `--steps 20 --vae-tiling` | zusätzliche Argumente |

Alle Felder sind normale Textfelder (Pfade mit Backslashes eintippen oder einfügen).

---

## 10. Automatisierung

Steuerkanal / MCP / CLI nutzen dieselben Befehls-IDs:

```json
{"method":"engine.execute","params":{"command":"ai.pingBackend","params":{}}}
{"method":"engine.execute","params":{"command":"ai.removeObject","params":{"prompt":"bare wall","strength":0.85,"output":"newLayer","seed":7}}}
{"method":"engine.execute","params":{"command":"ai.skinEnhance","params":{"strength":60,"smoothRadius":6,"preserveDetail":true}}}
{"method":"engine.execute","params":{"command":"ai.removeObject","params":{"output":"newLayer","source":"surroundings"}}}
```

Lange Befehle laufen als Job; `"wait": false` gibt sofort `{job, pending:true}` zurück, Fortschritt
über `jobs.list`, Abbruch mit `jobs.cancel`. Ein abgebrochener Job lässt das Dokument unverändert.

Einstellungen per Automatisierung:

```json
{"method":"engine.execute","params":{"command":"prefs.set","params":{"path":"ai.sdCli","value":"F:\\AI\\stable-diffusion-cpp\\sd-cli.exe"}}}
```

---

## 11. Fehlerbehebung

| Meldung / Symptom | Ursache | Lösung |
|---|---|---|
| „sd-cli not found at …“ | Pfad falsch | `sdCli` auf die EXE zeigen lassen |
| „no diffusion model configured“ | `Diffusion Model` leer | GGUF-Pfad eintragen |
| „sd-cli failed (exit …)“ mit Logauszug | Modell/VAE/Encoder fehlen, falscher Typ, CUDA-OOM | Pfade prüfen; kleineres Quant (Q3), T5 Q3, `--vae-tiling` |
| „sd-cli did not finish within … s“ | erster Lauf lädt das Modell | `Local timeout secs` auf 300–600 |
| „region is X MP, exceeds the 24.0 MP AI cap“ | Auswahl zu groß | kleiner auswählen oder `Max region mpx` erhöhen |
| „layer has no pixel surface“ | Text/Form/Gruppe/Smart Object | Ebene rastern oder Pixel-Ebene wählen |
| „no active selection“ | Remove Object ohne Auswahl | Auswahl aufziehen |
| „ComfyUI upload: HTTP 400“ | Workflow falsch (Node fehlt) / Server nicht bereit | API-Workflow prüfen, ComfyUI-Log ansehen |
| „ComfyUI: no output for prompt …“ | kein `SaveImage`-Node oder Workflow hängt | `SaveImage` ergänzen, Timeout erhöhen |
| „missing `response` field“ | Server ist ein Chat-/Ollama-Server | sd-cli oder ComfyUI konfigurieren |
| Test Connection zeigt rot, aber nichts sichtbar | alte EXE / Statusleiste aus | neue Version nutzen; Ergebnis steht jetzt direkt unter dem Knopf |
| Datei „workflow …: No such file“ | `ComfyUI Workflow` zeigt ins Leere | API-JSON exportieren und Pfad setzen |

---

## 12. Grenzen und Performance

- **Nur Inpainting** über sd-cli und ComfyUI; Segmentation (Skin Enhance vom Modell) bleibt offen.
- **24 MP** Standard-Obergrenze pro Auftrag (Einstellung `Max region mpx`).
- sd-cli startet pro Auftrag neu → Modell wird jedes Mal geladen; bei FLUX Q4 einige Sekunden bis
  eine Minute. ComfyUI hält das Modell im VRAM und ist für Serien deutlich schneller.
- 16 GB VRAM: FLUX Fill Q4 + T5 Q4 passt; parallel geöffnete Spiele/Browser belegen VRAM mit.
- Ergebnisbilder werden auf die Auswahl zurückgerechnet (nächster Nachbar), kleine
  Größenunterschiede sind daher normal.
- Lizenz beachten: FLUX.1 Fill **dev** ist nicht kommerziell; SDXL/SD1.5-Inpainting-Modelle haben
  eigene Lizenzen.

---

## 13. Wo es im Code liegt

| Thema | Datei |
|---|---|
| Backend-Auswahl, Befehle | `crates/engine/src/ai_cmds.rs` |
| Einstellungen | `crates/engine/src/prefs.rs` (`AiPrefs`) |
| sd-cli-Backend | `crates/ml/src/sdcli.rs` |
| ComfyUI-Backend | `crates/ml/src/comfy.rs` |
| Bildgrößen-Anpassung | `crates/ml/src/pixels.rs` |
| Panel | `crates/ui-egui/src/ai_props_ui.rs` |
| Anforderungsdokument | `docs/requirements-ai-inline-editing.md` |
