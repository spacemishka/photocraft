# PhotoCraft Active Context

## Current Status
- **Version:** 0.6.0 (Alpha stage).
- **Readiness:** ~45% ready for real work (Alpha gate passed for all 6 core daily workflows).
- **Parity Score:** Menu wiring 634/634 (100%), feature breadth ~76%, 56/68 tools, 30/35 panels.

## Current Priorities (from Roadmap & Gaps)
1. **G1: PSD Acceptance by Photoshop:**
   - **First: update the checkout.** `upstream/main` (storytold/photocraft) is 3,510 commits ahead of
     this branch and already carries the Photoshop text/Txt2 acceptance fixes: #2427 (unedited save
     `Txt2` byte-identical), #3529 (no fresh-layer stub), #4041 (TySh bounds/AntiAlias/version),
     #5673 (layer block after merged-transparency marker), #7220/#7698 (edited type keeps its `Txt2`
     object). #5157 (complete edited-text round trips) is still an open upstream PR; #6311 (global
     block padding) is closed upstream with no matching commit.
   - Remaining after the update: raise the Photoshop oracle floor (188/256 → 200+); the corpus is
     not fetched on this machine (`cargo xtask corpus --all`).
2. **G2: Performance Budgets:**
   - Address the 22 over-budget performance scenarios in `perf/budgets.toml`.
   - Resolve P12 GPU crash at 150 layers and brush/cursor latency (#2619, #2566).
3. **G3: Bug Backlog Triage:**
   - Resolve high-priority regressions and user-reported bugs from 0.5.x/0.6.0 releases.
4. **G4 & G5: Brush & Transform Feel:**
   - Bring brush dynamics, stroke interpolation, transform handles, and modifier behaviors up to 1:1 Photoshop feel.

## Active Rules & Development Notes
- Independent target directories per agent (`CARGO_TARGET_DIR=target/agent-<name>`).
- All changes must pass `cargo xtask layers` and maintain wasm compatibility.
- Ensure all command inputs are treated as untrusted and never cause panics.

## Current Work: AI Inline Editing (docs/requirements-ai-inline-editing.md)
- **MVP-1 done**: `photocraft-ml` L4 crate, `AiPrefs`, `ai.removeObject` / `ai.skinEnhance` / `ai.pingBackend`.
- **MVP-2 done and verified (2026-10-24)**:
  - AI commands are background jobs: progress stages, Esc cancels, document unchanged on cancel.
  - Params docs in registry notation → schema dialogs with live proxy preview (fallback algorithms render without a server).
  - Before/After split preview toggle (draggable divider) in the filter preview window (`filter_preview_ui.rs`).
  - On-canvas AI progress card (`jobs_ui::ai_card`), centred on the canvas rect and drawn in a child UI; modal progress dialog suppressed for `ai.*`.
  - Menu rows: Edit › Content-Aware › Remove Object…, Filter › Neural Filters › Skin Enhance… (`menu_catalog.rs`); Edit › Preferences › AI… (`edit.preferences.ai`).
  - Properties panel › AI Retouching (`ai_props_ui.rs`, `UiState.ai`): backend test, Skin Enhance + Remove Object controls ("Output to a new layer").
- **Verification (2026-10-24):** engine lib 1700 passed; ui-egui lib 2966 passed; clippy `-D warnings` clean (engine, ui-egui, ml); `cargo xtask wasm` 25/25 ok; `panic_hunt --ignored` ok; `xtask parity` 634/634 (FLOOR raised); `xtask scorecard` regenerated (never-crash lints now 30/30).
- **Next steps:** nothing left for MVP-2. Ranked focus returns to **G1 PSD acceptance** — fix Photoshop rejections (#1281, #2469) and raise the Photoshop oracle floor (≥188/256 in the current scorecard) toward 200+.

