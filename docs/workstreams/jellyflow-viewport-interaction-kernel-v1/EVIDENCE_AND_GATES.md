# Jellyflow Viewport Interaction Kernel v1 - Evidence And Gates

Status: Closed
Last updated: 2026-06-01

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime adapter_conformance::adapter_conformance_viewport_and_selection_emit_ordered_view_changes
```

This currently proves direct viewport and selection view-change ordering. JVI-040 should move
viewport conformance through the fixture runner where appropriate.

## Gate Set

### Viewport Kernel Gate

```bash
cargo nextest run -p jellyflow-runtime viewport
cargo check -p jellyflow-runtime
```

This proves viewport pan/zoom helpers and store wiring.

### Conformance Gate

```bash
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo check -p jellyflow-runtime
```

This proves viewport fixture traces and adapter-conformance coverage.

### Broader Closeout Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-viewport-interaction-kernel-v1/WORKSTREAM.json
git diff --check
```

This proves formatting, runtime behavior, lint cleanliness, JSON validity, and diff hygiene.

## Evidence Anchors

- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `docs/workstreams/jellyflow-conformance-fixtures-v1/CLOSEOUT_AUDIT_2026-06-01.md`
- `docs/workstreams/jellyflow-node-drag-kernel-v1/CLOSEOUT_AUDIT_2026-06-01.md`
- `docs/workstreams/jellyflow-geometry-spatial-v1/CLOSEOUT_AUDIT_2026-06-01.md`
- `README.md`
- `crates/jellyflow-runtime/README.md`

## Fresh Evidence Log

- 2026-06-01: JVI-010 opened the viewport interaction kernel workstream.
  - `git status --short --branch`: clean before opening docs, branch ahead of origin.
  - Governing decision: ADR 0003 keeps viewport interaction headless and renderer-free.
  - Source evidence: conformance fixture closeout, node drag kernel closeout, and geometry spatial
    closeout.
- 2026-06-01: JVI-020 added renderer-neutral viewport transform helpers.
  - `cargo nextest run -p jellyflow-runtime viewport`: 12 passed, 144 skipped.
  - `cargo nextest run -p jellyflow-runtime explicit_modules_expose_their_owned_surfaces`: 1
    passed, 155 skipped.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo fmt --check`: passed.
  - Public API: `runtime::viewport::{ViewportTransform, ViewportPanRequest,
    ViewportZoomRequest, pan_viewport, zoom_viewport}`.
- 2026-06-01: JVI-030 wired viewport intent through store publication and callback lifecycle.
  - `cargo nextest run -p jellyflow-runtime viewport`: 14 passed, 144 skipped.
  - `cargo nextest run -p jellyflow-runtime explicit_modules_expose_their_owned_surfaces`: 1
    passed, 157 skipped.
  - `cargo nextest run -p jellyflow-runtime adapter_conformance`: 8 passed, 150 skipped.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo fmt --check`: passed.
  - `jq empty docs/workstreams/jellyflow-viewport-interaction-kernel-v1/WORKSTREAM.json
    docs/workstreams/jellyflow-viewport-interaction-kernel-v1/TASKS.jsonl`: passed.
  - `git diff --check`: passed.
  - Store API: `NodeGraphStore::apply_viewport_pan` and `apply_viewport_zoom`.
  - Gesture/callback API: `ViewportMoveStart`, `ViewportMove`, `ViewportMoveEnd`, and
    `NodeGraphGestureCallbacks::on_move`.
- 2026-06-01: JVI-040 added viewport conformance fixture coverage.
  - `cargo nextest run -p jellyflow-runtime conformance`: 12 passed, 147 skipped.
  - `cargo nextest run -p jellyflow-runtime adapter_conformance`: 8 passed, 151 skipped.
  - `cargo check -p jellyflow-runtime`: passed.
  - Fixture vocabulary: `ConformanceAction::apply_viewport_pan`,
    `ConformanceAction::apply_viewport_zoom`, viewport view callback trace events, and viewport
    move callback trace events.
  - Adapter viewport/selection ordering now runs through `run_conformance_scenario`.
- 2026-06-01: JVI-050 closed the viewport interaction kernel workstream.
  - `cargo fmt --check`: passed.
  - `cargo nextest run -p jellyflow-runtime`: 159 passed, 0 skipped.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `jq empty docs/workstreams/jellyflow-viewport-interaction-kernel-v1/WORKSTREAM.json`: passed.
  - `git diff --check`: passed.
  - Documentation: `README.md`, `crates/jellyflow-runtime/README.md`, and
    `CLOSEOUT_AUDIT_2026-06-01.md`.

## Notes

- Fresh verification is required before marking a task, Codex goal, or lane complete.
- Runtime accepts normalized viewport intent; adapters own raw wheel, touch, pointer, and platform
  event capture.
- Keep animation, smoothing, screenshot, pixel, GPU, and windowing behavior outside
  `jellyflow-runtime` in v1.
