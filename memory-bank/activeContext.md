# PhotoCraft Active Context

## Current Status
- **Version:** 0.15.1 (branch `ai-inline-editing`, synced with `origin/ai-inline-editing`).
- **Parity score:** menu wiring 638/638 (100%), FLOOR 638 in `crates/ui-egui/src/parity.rs`.
- **Upstream:** merged storytold main at `1c3db2e5c` (2026-10-24); the PSD text/Txt2 acceptance
  fixes (#2427, #3529, #4041, #5673, #7220, #7698) are local. #5157 (edited-text round trips) is
  still an upstream PR; #6311 (global block padding) is closed upstream with no named commit.
- **Corpus:** not fetched on this machine; G1 needs `cargo xtask corpus --all`.

## Current Priorities (from docs/gaps.md)
1. **G1: PSD acceptance by Photoshop** — run the corpus oracles (`cargo xtask test-corpus`) and
   work the remaining render/round-trip failures; Photoshop oracle floor lives in
   `crates/io/tests/corpus.rs`.
2. **G2: Performance budgets** — 22 over-budget scenarios in `perf/budgets.toml`; P12 GPU crash at
   150 layers, brush/cursor latency (#2619, #2566).
3. **G3: Bug backlog** — high-priority regressions from 0.5.x/0.6.0.
4. **G4/G5: Brush and transform feel** — dynamics, interpolation, handles, modifiers.

## Recently completed (2026-10-24 / 25, this branch)
- **AI inline editing MVP-2 verified.** Engine + ui-egui suites, clippy `-D warnings` (engine,
  ui-egui, ml), `xtask wasm`, `panic_hunt --ignored` all green. Fixes on top of `mvp2 init`:
  `PixelFormat::GRAY32F`, `edit.preferences.ai`, AI card drawn in a child UI on the canvas rect,
  `left_of_split` takes a 0..1 fraction, `Label::truncate()`, unique "Output to a new layer" label,
  `ml/native` only for non-wasm targets.
- **Local AI backends** (docs/requirements-ai-inline-editing.md §6/§7):
  - `crates/ml/src/comfy.rs`: upload source+mask, patch an API-format workflow through
    `{{image}}`, `{{mask}}`, `{{prompt}}`, `{{negative_prompt}}`, `{{seed}}`, `{{strength}}`,
    `{{width}}`, `{{height}}`, `{{task}}`; `/prompt` -> `/history` -> `/view`.
  - `crates/ml/src/sdcli.rs`: one `sd-cli` process per request (`-i`, `--mask`,
    `--diffusion-model`, `--vae`, `--clip_l`, `--t5xxl`, extra args), temp PNGs, timeout kill,
    `CREATE_NO_WINDOW`.
  - Priority `sdCli` > `comfyWorkflow` > `localUrl`; `ai.pingBackend` names the backend.
  - `crates/ml/src/pixels.rs` resamples server images to the region (a mismatched resolution used
    to be able to panic in `write_interleaved`).
- **AI panel UX**: prompt/strength/checkbox bind to `app.ui.ai` directly (they snapped back before);
  the hint is editable without a selection; Test Connection shows its result inline; the header
  row names the active backend.
- **Remove Object** gained `source: "model" | "surroundings"` (classical Content-Aware fill, no
  backend, much faster). Panel checkbox "Replace with surrounding background (no AI)".
- **Skin Enhance** defaults to `output: "newLayer"` (`AI: Skin Enhance 1`); `inPlace` keeps the old
  behaviour.
- **Noise analysis** (`analysis.noise`, `crates/engine/src/noise_cmds.rs`): Immerkær residual over
  <=1.5M samples, per-channel and luma sigma, flat-area grain (quiet decile of 16x16 blocks),
  chroma, JPEG blockiness, clipped shadows/highlights. `analysis.noise.reduce` opens Reduce Noise
  with all five recommended values (`filter_dialog::open_with`). Docs:
  `docs/noise-analysis.md`.
- **Auto Correct** (`image.autoCorrect`, `crates/engine/src/auto_correct_cmds.rs`): a plain Levels
  layer (default name) with each colour channel's empty margins trimmed, plus a Vibrance layer for
  coloured images below 55% mean saturation; `strength` 0-100. Docs: `docs/auto-correct.md`.
- **Grain extraction** (`image.extractGrain`, `crates/engine/src/grain_cmds.rs`): residual
  `pixel - blur`, band-passed and with a dilated edge mask (neutral 0.5), new "Film Grain" layer in
  Overlay. Reusable as a normal layer. Docs: `docs/grain-extraction.md`.
- **User guide:** `docs/ai-retouching-guide.md` (German), linked from the requirements document.

## Notes for the next session
- **UI command routing:** menu commands that open a dialog/action live in `wraps()` in
  `crates/ui-egui/src/view_cmds.rs` (with an arm in `front()`); `handles()` is only for view
  toggles handled by `run()`. Putting an id in the wrong list yields "unhandled <id>".
- **`local_blur` and residuals:** pass `alpha: None` when blurring a residual. With an alpha index,
  pixels whose alpha is ~0 fall back to the source value (line 179), so a band-pass over a residual
  (alpha 0) returns the input unchanged.
- **Windows relink:** `cargo build -p photocraft` fails with "Zugriff verweigert" while the app
  runs; close PhotoCraft first. Long builds survive server restarts only when run in the background.
- **Defender ASR:** fresh build-script exes may be blocked for ~90 s; keep the same
  `CARGO_TARGET_DIR` and retry (see `techContext.md`).
- **New menu rows** raise the parity count: run `cargo xtask parity`, then raise `FLOOR` and run
  `cargo xtask scorecard` before committing.
