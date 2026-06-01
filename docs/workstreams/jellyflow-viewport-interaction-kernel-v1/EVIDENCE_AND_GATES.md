# Jellyflow Viewport Interaction Kernel v1 - Evidence And Gates

Status: Active
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

## Notes

- Fresh verification is required before marking a task, Codex goal, or lane complete.
- Runtime accepts normalized viewport intent; adapters own raw wheel, touch, pointer, and platform
  event capture.
- Keep animation, smoothing, screenshot, pixel, GPU, and windowing behavior outside
  `jellyflow-runtime` in v1.
