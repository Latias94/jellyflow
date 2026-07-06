---
type: Work Progress
title: Open GPUI Mindmap readiness fix
timestamp: 2026-07-06T13:40:08Z
tags:
  - open-gpui
  - mindmap
  - node-components
  - frame-scheduling
status: verified
git_branch: feat/xyflow-product-surface
source_workspace: /Users/frankorz/codes/rust/jellyflow
---

# Summary

Mindmap product fixtures no longer depend on pointer movement to finish node UI rendering. The
root cause had two parts:

- The Mindmap fixture projected `demo.idea` nodes with renderer key `idea-card`, but the Open GPUI
  canvas-jellyflow example only registered `topic-card` and `source-card`. Those nodes fell back to
  descriptor rendering, so the fixture mixed product renderers and fallback surfaces.
- `schedule_measurement_frame` registered an `on_next_frame` callback without immediately requesting
  a window refresh. If the callback was registered during render or draw, the next frame could wait
  for a pointer/window event, which matched the observed "mouse movement completes the UI" symptom.

# Changes

- Added a headless layout budget for `demo.idea` in
  `crates/jellyflow-runtime/src/schema/kit/builtins.rs`.
- Updated the Open GPUI product fixture spec in
  `crates/jellyflow-open-gpui/src/testing.rs` to require `idea-card`.
- Registered and implemented the local Open GPUI `idea-card` renderer in
  `repo-ref/open-gpui/examples/canvas-jellyflow/src/product_renderers.rs`.
- Added readiness-aware measurement scheduling in
  `repo-ref/open-gpui/examples/canvas-jellyflow/src/main.rs`.
- `schedule_measurement_frame` now calls `window.refresh()` immediately after registering the
  next-frame callback, so measurement work is self-driving even without pointer events.
- Pulled `repo-ref/open-gpui` local `main` from `origin/main`, resolved conflicts by preserving
  both the local quit-mode test support and upstream cursor/system-wake support, and re-enabled
  `examples/canvas-jellyflow` as an Open GPUI workspace member so its `workspace = true` manifest
  remains valid.

# Commits

- `repo-ref/open-gpui`: `dd95d04e chore: merge origin main into local main`
- `repo-ref/open-gpui`: `3b5e31ef fix(jellyflow): complete mind map product rendering`

# Next Action

Ask the user to retest Mindmap in the native example. If there is still delayed rendering, inspect
the Open GPUI scene presentation path rather than adding more size heuristics.

# Citations

- `crates/jellyflow-runtime/src/schema/kit/builtins.rs`
- `crates/jellyflow-open-gpui/src/testing.rs`
- `repo-ref/open-gpui/examples/canvas-jellyflow/src/main.rs`
- `repo-ref/open-gpui/examples/canvas-jellyflow/src/product_renderers.rs`
