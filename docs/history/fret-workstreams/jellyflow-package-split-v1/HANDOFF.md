# Jellyflow Package Split v1 - Handoff

Status: Closed
Last updated: 2026-05-30

## Current State

JF-020 is complete. `jellyflow-core` owns the stable graph model, interaction, type, ops, and
transaction/history helpers. `jellyflow-runtime` now owns the headless `io`, `profile`, `rules`,
`schema`, and `runtime` modules. `fret-node` depends on both Jellyflow crates, preserves old module
paths with compatibility re-exports, and keeps Fret UI/kit/profile recipes in the adapter crate.

JF-030 is also complete. The audit concluded that canvas-space geometry, route math, spatial
indexes, and hit-test helpers should stay in `fret-node` for now. `jellyflow-geometry` remains a
future slot only, not a live package boundary.

## Active Task

- None. The Jellyflow package-split lane is closed.

## JF-001 Evidence

- `cargo check -p jellyflow-core`: passed.
- `cargo nextest run -p jellyflow-core`: passed with 14 tests.
- `cargo clippy -p jellyflow-core --all-targets -- -D warnings`: passed.
- `cargo check -p fret-node --all-features --tests`: passed.
- `cargo nextest run -p fret-node --no-default-features`: passed with 124 tests.
- `cargo fmt --check`: passed.
- `jq empty docs/workstreams/jellyflow-package-split-v1/WORKSTREAM.json`: passed.
- `git diff --check`: passed.
- `python3 tools/check_layering.py`: passed.

## JF-010 Evidence

- `cargo check -p fret-node --all-features --tests`: passed after moving `ops` into
  `jellyflow-core`.
- `cargo nextest run -p fret-node --no-default-features`: passed with 90 tests after the `ops`
  split.
- `cargo fmt --check`: passed after rustfmt cleanup.
- `python3 tools/check_layering.py`: passed after the `ops` split.

## JF-020 Evidence

- `cargo check -p jellyflow-runtime`: passed.
- `cargo nextest run -p jellyflow-runtime`: passed with 67 tests.
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
- `cargo check -p fret-node --all-features --tests`: passed.
- `cargo nextest run -p fret-node --no-default-features`: passed with 24 tests.
- `cargo fmt --check`: passed.

## Next Steps

1. Start a new follow-on for standalone repository extraction, publishing, or compatibility
   re-export deprecation/removal.
2. Keep geometry and spatial helpers in `fret-node` until a real second consumer appears.
3. If the seam reopens later, start by extracting only the smallest pure math subset, not the whole
   adapter canvas stack.
4. Add focused compatibility gates before any future move that crosses `NodeGraphStyle`,
   `NodeGraphPresenter`, or `CanvasSpatialDerived`.
