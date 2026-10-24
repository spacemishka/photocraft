# Requirements & Software Requirements Specification: AI Inline Editing

> **Document ID:** SRS-AI-INLINE-001  
> **Status:** Draft / Active Implementation  
> **Scope:** Local Server First (Ollama, llama.cpp), Remote Hosted VLM Deferred  
> **Target:** PhotoCraft Engine & UI

---

## 1. Executive Summary

This specification defines inline AI-assisted image editing inside PhotoCraft without cloud dependencies. Editing operations run against configurable local LLM/VLM inference servers (such as Ollama or `llama.cpp` `--server`), with zero data egress. When local servers are offline or unconfigured, operations fall back gracefully to deterministic classical algorithms (PatchMatch inpainting, bilateral/Gaussian smoothing).

Remote hosted VLMs (OpenAI, Stability AI, Replicate) are specified in later phases (MVP-4+) and remain dormant until explicitly enabled by user configuration.

---

## 2. Core Architecture & Layering

Following PhotoCraft's strict architectural layering:

```text
L6  ui-egui (Preferences dialog › AI, on-canvas feedback, before/after toggle)
L5  photocraft-engine (Commands: ai.removeObject, ai.skinEnhance, ai.pingBackend)
L4  photocraft-ml (InferenceBackend trait, LocalBackend, ClassicalFallback)
L0  photocraft-codecs, photocraft-raster, photocraft-color, photocraft-geom
```

- **Layer 4 (`photocraft-ml`):** Pure-Rust backend interface; HTTP client feature-gated behind `native` (no network on wasm32 targets).
- **Layer 5 (`photocraft-engine`):** User operations are registered as standard commands (`ai_cmds.rs`), completely callable through GUI, CLI, and MCP automation.
- **Fail-safe:** If inference fails or the server is disconnected, the engine seamlessly uses built-in CPU reference algorithms (`content_aware_native`, `local_blur`).

---

## 3. MVP Milestones

### MVP-1: Local Server Foundation (Current Scope)
- [x] Create `photocraft-ml` crate (L4) with `InferenceBackend` trait.
- [x] Implement `LocalBackend` with Ollama / llama.cpp REST client.
- [x] Implement base64 PNG encode/decode using `photocraft-codecs` and `photocraft-raster`.
- [x] Implement `ClassicalFallback` returning `MlError::Unsupported`.
- [x] Add `AiPrefs` to Edit › Preferences (`localUrl`, `localModel`, `localTimeoutSecs`, `maxRegionMpx`).
- [x] Implement `ai.removeObject` command: selection-masked inpainting with classical PatchMatch fallback.
- [x] Implement `ai.skinEnhance` command: skin detection + frequency separation smoothing with classical fallback.
- [x] Implement `ai.pingBackend` diagnostic command for connectivity checks.

### MVP-2: Interactive UI & On-Canvas Polish
- [x] On-canvas progress overlay for running inference jobs.
- [x] Cancel support (Escape key aborts in-flight local inference).
- [x] Before/After split preview toggle for AI filters.
- [x] Properties panel options bar controls for AI retouching tools.

### MVP-3: Local Specialized Vision Models
- [ ] Bundled / locally hosted SAM2 (Segment Anything) promptable segmentation.
- [ ] Local Real-ESRGAN / super-resolution upscaler integration.
- [ ] Direct GGUF loading or ONNX Runtime execution via local provider.

### MVP-4: Hosted VLM / Cloud Providers (Deferred)
- [ ] Remote provider abstraction (OpenAI, Stability AI, Fal.ai).
- [ ] API key vault via OS Keychain.
- [ ] Explicit opt-in consent dialog before any pixel data leaves the machine.

---

## 4. Engine Command Specifications

### 4.1 `ai.removeObject`
- **ID:** `ai.removeObject`
- **Menu:** `Edit` › `Content-Aware` › `Remove Object...`
- **Parameters:**
  ```json
  {
    "layer": 1,
    "prompt": "clear background",
    "seed": 42,
    "strength": 0.85,
    "output": "newLayer",
    "source": "model"
  }
  ```
- **Behavior:**
  1. Validates selection presence and checks region bounds against `maxRegionMpx`.
  2. Encodes active layer region and selection mask to base64 PNG.
  3. Sends request to local server (`POST /api/generate`).
  4. If server unreachable, invokes `content_aware_native` (classical PatchMatch).
  5. Commits result non-destructively as a new raster layer above the active layer (or in-place if `"output": "inPlace"`).

`"source": "surroundings"` skips steps 2–4 entirely: the selection is filled from the pixels
around it with `content_aware_native` (Photoshop's Content-Aware Fill), no model and no prompt
needed. `"source": "model"` (default) uses the configured backend.

### 4.2 `ai.skinEnhance`
- **ID:** `ai.skinEnhance`
- **Menu:** `Filter` › `Neural Filters` › `Skin Enhance...`
- **Parameters:**
  ```json
  {
    "layer": 1,
    "strength": 50,
    "smoothRadius": 6,
    "preserveDetail": true,
    "output": "newLayer"
  }
  ```
- **Behavior:**
  1. Extracts bounding region of active pixel layer.
  2. Attempts segmentation via local backend; falls back to adaptive HSV skin-luma heuristic mask.
  3. Decomposes image frequencies using Gaussian local blur.
  4. Smooths low-frequency blemishes while keeping high-frequency texture when `preserveDetail` is active.
  5. Commits the result as a new layer above the source (`"output": "newLayer"`, the default,
     named "AI: Skin Enhance 1") or into the source layer (`"output": "inPlace"`), with history.

### 4.3 `ai.pingBackend`
- **ID:** `ai.pingBackend`
- **Menu:** (None, diagnostic/preferences only)
- **Parameters:** `{}`
- **Behavior:** Checks local server endpoint (`/api/tags`) and returns connectivity status and latency.

---

## 5. Configuration & Preferences

Benutzerhandbuch: [ai-retouching-guide.md](ai-retouching-guide.md).

Preferences stored under `ai`:
- `localUrl` (string, default: `"http://127.0.0.1:11434"`): Local Ollama or llama.cpp endpoint.
- `localModel` (string, default: `"llava"`): Model tag for vision/inpaint tasks.
- `localTimeoutSecs` (u32, default: `120`): Maximum time to wait for local generation.
- `maxRegionMpx` (f32, default: `24.0`): Guard against OOM on massive canvas areas.
- `comfyUrl` (string, default: `"http://127.0.0.1:8188"`): Local ComfyUI server.
- `comfyWorkflow` (string, default: `""`): Path to a ComfyUI workflow in API format. **Empty uses the
  Ollama-style `localUrl` server**; non-empty switches every AI command to ComfyUI.
- `sdCli`, `sdModel`, `sdVae`, `sdClipL`, `sdT5xxl`, `sdArgs`: the stable-diffusion.cpp route. A
  non-empty `sdCli` wins over `comfyWorkflow`; see §7.

## 6. ComfyUI backend (`crates/ml/src/comfy.rs`)

ComfyUI runs the diffusion models (FLUX.1 Fill, SDXL inpainting) that a llama.cpp server cannot.
Export the workflow with **Workflow ▸ Export (API)**, then use these placeholders anywhere in a
string value; the backend patches the JSON before queueing it:

| Placeholder | Value |
|---|---|
| `{{image}}`, `{{mask}}` | input names of the uploaded source/mask PNG (white = repaint) |
| `{{prompt}}`, `{{negative_prompt}}` | the command's text |
| `{{seed}}` | number (`KSampler.seed`) |
| `{{strength}}` | number 0..1 (`KSampler.denoise`) |
| `{{width}}`, `{{height}}` | the selected region's pixel size |
| `{{task}}` | `inpaint` |

A value that is exactly a numeric placeholder becomes a JSON number, so it can feed `KSampler`
and `EmptyLatentImage` nodes directly. Flow: `POST /upload/image` (source and mask) →
`POST /prompt` → poll `GET /history/{id}` → `GET /view` for the first output image; the result is
resampled to the selection. `ai.pingBackend` uses `GET /system_stats` and reports
`"backend": "comfyui"`. Segmentation is not supported by this backend, so Skin Enhance keeps its
classical HSV mask and only smooths.

FLUX.1 Fill dev on 16 GB: `flux1-fill-dev-Q4_0.gguf` (≈6.9 GB) + `ae.safetensors` + `clip_l` +
`t5xxl` Q4_K_M (≈3.1 GB), all from `second-state/FLUX.1-Fill-dev-GGUF` and
`comfyanonymous/flux_text_encoders`; the model is non-commercial (`FLUX.1 [dev]` licence).

## 7. stable-diffusion.cpp backend (`crates/ml/src/sdcli.rs`)

`sdCli` (the path to `sd-cli.exe`) selects this backend; it has no server and spawns one process
per request. Files: `sdModel` (`--diffusion-model`, e.g.
`F:\AI\models\flux-fill\flux1-fill-dev-Q4_0.gguf`), `sdVae` (`ae.safetensors`), `sdClipL`
(`clip_l.safetensors`), `sdT5xxl` (FLUX needs it, Q4_K_M is enough), `sdArgs` (default
`--steps 20 --vae-tiling`; for FLUX add `--guidance 30 --cfg-scale 1 --sampling-method euler`).

The command is `sd-cli --diffusion-model … --vae … -i source.png --mask mask.png -o out.png
-W <w> -H <h> -p "…" --strength <s>`, with white in the mask marking the area to repaint. The
region is rounded up to a multiple of 32 for the model and resampled back. The process is killed
after `localTimeoutSecs` (raise it for a cold FLUX load). A missing binary or model is reported as
a configuration error; an unreachable/misconfigured run does not silently fall back.



