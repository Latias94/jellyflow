# Jellyflow Geometry Spatial v1 - Handoff

Status: Closed
Last updated: 2026-06-01

## Current State

The workstream is open as the next fearless-refactor lane after the closed runtime public-surface and
model-policy lanes.

JGS-010 is complete: the lane scope, non-goals, source coverage, task ledger, milestones, gate set,
context manifest, and machine-readable workstream metadata are recorded.

JGS-020 is complete: duplicated fit-view and runtime-utils bounds math now routes through
`runtime::geometry`. The module remains crate-private while the contract settles.

JGS-030 is complete: `NodeGraphLookups` exposes sorted parent/child queries derived from
`node_lookup`, and runtime node-inside tests cover deterministic linear fallback, hidden nodes, and
fallback sizes. A real indexed spatial backend is deferred.

JGS-040 is complete: `runtime::geometry` is now public for renderer-neutral endpoint geometry, while
internal bounds helpers remain crate-private. Adapters can pass node-local handle rects and receive
canvas-space endpoint points plus side metadata.

JGS-050 is complete: `runtime::geometry` includes renderer-neutral path commands for straight,
bezier, and smoothstep-like edges, plus numeric path-distance hit testing wired to
`NodeGraphInteractionState`.

JGS-060 is complete: root/runtime READMEs mention the geometry surface, and
`crates/jellyflow-runtime/examples/geometry_edge.rs` demonstrates endpoint/path/hit-test usage.

JGS-070 is complete: the lane is closed after full workspace verification.

## Final Gates

- `cargo fmt --check`: passed.
- `cargo nextest run --workspace`: passed, 207 tests.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `python3 tools/check_no_fret_dependencies.py`: passed.
- `python3 tools/check_external_consumer_smoke.py`: passed.
- `jq empty docs/workstreams/jellyflow-geometry-spatial-v1/WORKSTREAM.json`: passed before closeout edits.
- `git diff --check`: passed before closeout edits.

## Decisions Since Last Update

- Chose geometry/spatial as the next fearless-refactor lane instead of schema migration or XyFlow compatibility deletion.
- Chose a module-first approach inside `jellyflow-runtime` instead of immediately creating a third crate.
- Chose duplicated bounds/fit-view/runtime utility math as the first proof slice.
- Added crate-private `runtime::geometry` with `CanvasBounds`, node-origin projection, and
  `ViewportFitFrame`.
- Kept JGS-020 behavior-preserving: public `runtime::fit_view` and `runtime::utils` APIs did not
  change.
- Added parent/child lookup methods as derived queries instead of adding a new public field to
  `NodeGraphLookups`.
- Kept current node spatial queries as deterministic linear scans; documented
  `NodeGraphSpatialIndexTuning` as reserved for a future indexed backend.
- Made `runtime::geometry` public only after adding concrete endpoint primitives needed by adapters.
- Kept edge endpoint output renderer-neutral: points and handle sides, not SVG strings.
- Added backend-neutral edge paths as command data and hit testing as numeric distance checks.
- Wired existing `edge_interaction_width` and `bezier_hit_test_steps` config into
  `NodeGraphInteractionState::edge_hit_test_options`.
- Documented public geometry in README files and added the `geometry_edge` runtime example.
- Kept DOM/d3, renderer bindings, drag/panzoom/resize gesture state machines, and persisted schema movement out of scope.
- Recorded `cargo nextest run --workspace` baseline: 191 tests passed on 2026-06-01.
- Recorded JGS-020 evidence: `cargo fmt --check`, `cargo nextest run -p jellyflow-runtime runtime::geometry`, `cargo nextest run -p jellyflow-runtime runtime::fit_view`, `cargo nextest run -p jellyflow-runtime runtime::tests::utils`, and `cargo check -p jellyflow-runtime` passed.
- Recorded JGS-030 evidence: `cargo fmt --check`, `cargo nextest run -p jellyflow-runtime runtime::tests::lookups`, `cargo nextest run -p jellyflow-runtime runtime::tests::utils`, and `cargo check -p jellyflow-runtime` passed.
- Recorded JGS-040 evidence: `cargo fmt --check`, `cargo nextest run -p jellyflow-runtime edge_position`, and `cargo check -p jellyflow-runtime` passed.
- Recorded JGS-050 evidence: `cargo fmt --check`, `cargo nextest run -p jellyflow-runtime edge_path`, `cargo nextest run -p jellyflow-runtime hit_test`, and `cargo check -p jellyflow-runtime` passed.
- Recorded JGS-060 evidence: `cargo fmt --check`, `cargo nextest run -p jellyflow-runtime`, `cargo check -p jellyflow-runtime`, and `cargo run -p jellyflow-runtime --example geometry_edge` passed.
- Recorded JGS-070 closeout evidence with full workspace tests, clippy, dependency boundary checks,
  external consumer smoke, JSON validation, and diff hygiene.

## Blockers

- None known.

## Follow-On Candidates

- Real spatial-index backend behind `NodeGraphSpatialIndexTuning` once adapter workloads prove the
  linear fallback is too slow.
- Full headless interaction kernels for drag, pan/zoom, resize, selection boxes, and reconnect
  gestures after geometry consumers settle.
- Optional adapter helpers for converting `PathCommand` to backend-specific path formats such as
  SVG, egui shapes, or Fret canvas primitives.
