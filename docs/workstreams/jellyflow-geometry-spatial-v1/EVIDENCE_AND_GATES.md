# Jellyflow Geometry Spatial v1 - Evidence And Gates

Status: Closed
Last updated: 2026-06-01

## Smallest Current Repro

```bash
rg -n "struct CanvasBounds|compute_fit_view_target|get_nodes_inside|spatial_index|edge_interaction_width|bezier_hit_test_steps" crates/jellyflow-runtime/src
```

This shows the current duplicated or partially-backed geometry/spatial surface: private bounds
types, fit-view math, node-inside scanning, and config fields that should either drive shared
behavior or be explicitly deferred.

## Gate Set

### Baseline Gate

```bash
cargo nextest run --workspace
```

This proves the current extracted Jellyflow workspace is green before geometry/spatial edits.

### Targeted Iteration Gates

```bash
cargo nextest run -p jellyflow-runtime runtime::fit_view
cargo nextest run -p jellyflow-runtime runtime::tests::utils
cargo nextest run -p jellyflow-runtime runtime::tests::lookups
cargo check -p jellyflow-runtime
```

These prove the first shared-geometry slices preserve fit-view, bounds, lookup, and public runtime
utility behavior.

### Package Gate

```bash
cargo nextest run -p jellyflow-runtime
```

This proves runtime-level behavior remains coherent after geometry/spatial integration.

### Dependency Boundary Gate

```bash
python3 tools/check_no_fret_dependencies.py
```

This proves geometry/spatial work does not reintroduce Fret dependencies.

### External Consumer Gate

```bash
python3 tools/check_external_consumer_smoke.py
```

This proves an outside project can still path-depend on Jellyflow without pulling Fret packages.

### Broader Closeout Gate

```bash
cargo fmt --check
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-geometry-spatial-v1/WORKSTREAM.json
git diff --check
```

This proves formatting, workspace behavior, lint cleanliness, workstream metadata, and diff hygiene.

### Review Gate

Run `review-workstream` before accepting task or lane completion. Run `verify-rust-workstream`
before marking the lane complete.

## Evidence Anchors

- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/adr/0002-jellyflow-model-policy-boundary.md`
- `docs/workstreams/jellyflow-runtime-public-surface-v1/CLOSEOUT_AUDIT_2026-05-30.md`
- `docs/workstreams/jellyflow-model-policy-boundary-v1/CLOSEOUT_AUDIT_2026-05-30.md`
- `repo-ref/xyflow/packages/system/src/utils/`
- `repo-ref/xyflow/packages/system/src/xydrag/`
- `repo-ref/xyflow/packages/system/src/xyhandle/`
- `repo-ref/xyflow/packages/system/src/xypanzoom/`
- `repo-ref/xyflow/packages/system/src/xyresizer/`
- `crates/jellyflow-runtime/src/runtime/fit_view/`
- `crates/jellyflow-runtime/src/runtime/utils/`
- `crates/jellyflow-runtime/src/runtime/lookups/`
- `crates/jellyflow-runtime/src/io/tuning/spatial_index.rs`
- `crates/jellyflow-runtime/src/io/config/interaction/config.rs`

## Fresh Evidence Log

- 2026-06-01: JGS-010 opened the geometry/spatial workstream after a fearless-refactor scan.
  - `git status --short --branch`: clean working tree before docs were written, branch ahead of origin.
  - `cargo nextest run --workspace`: passed with 191 tests.
  - Existing closed lanes reviewed: runtime public surface and model policy boundary.
  - No code changes in JGS-010.
- 2026-06-01: JGS-020 introduced the shared runtime geometry proof slice.
  - Changed `crates/jellyflow-runtime/src/runtime/geometry.rs` to own `CanvasBounds`, node-origin projection, rect union/intersection/contains, and `ViewportFitFrame`.
  - Routed `runtime::fit_view` and `runtime::utils::bounds` through the shared geometry primitives.
  - Deleted the duplicate `runtime::utils::geometry` private bounds type.
  - `cargo fmt --check`: passed.
  - `cargo nextest run -p jellyflow-runtime runtime::geometry`: passed, 3 tests.
  - `cargo nextest run -p jellyflow-runtime runtime::fit_view`: passed, 4 tests.
  - `cargo nextest run -p jellyflow-runtime runtime::tests::utils`: passed, 10 tests.
  - `cargo check -p jellyflow-runtime`: passed.
  - Review: self-review against `review-workstream` checklist found no blocking or important findings for JGS-020.
- 2026-06-01: JGS-030 added lookup-derived parent/child and deterministic spatial-query behavior.
  - Added `NodeGraphLookups::parent_for_node`, `child_nodes_for_parent`, `child_nodes_by_parent`, and `root_nodes` without changing the public struct fields.
  - Added tests for sorted parent lookup, transaction updates, sorted linear inside-rect results, hidden-node policy, and fallback-size policy for unsized nodes.
  - Documented `NodeGraphSpatialIndexTuning` as reserved for a future indexed backend; current node spatial queries remain deterministic linear scans with sorted output.
  - `cargo fmt --check`: passed.
  - `cargo nextest run -p jellyflow-runtime runtime::tests::lookups`: passed, 9 tests.
  - `cargo nextest run -p jellyflow-runtime runtime::tests::utils`: passed, 12 tests.
  - `cargo check -p jellyflow-runtime`: passed.
  - Review: self-review against `review-workstream` checklist found no blocking or important findings for JGS-030.
- 2026-06-01: JGS-040 added renderer-neutral handle and edge endpoint geometry.
  - Made `runtime::geometry` public while keeping internal bounds helpers crate-private.
  - Added `HandlePosition`, `HandleBounds`, `EdgeEndpointInput`, `EdgeEndpointPosition`, `EdgePosition`, `handle_anchor_position`, `handle_center_position`, and `edge_position`.
  - Added `NodeGraphLookups::connection_for_edge` so adapters can resolve an edge to source/target ports without reading raw maps.
  - `cargo fmt --check`: passed.
  - `cargo nextest run -p jellyflow-runtime edge_position`: passed, 5 tests.
  - `cargo check -p jellyflow-runtime`: passed.
  - Review: self-review against `review-workstream` checklist found no blocking or important findings for JGS-040.
- 2026-06-01: JGS-050 added backend-neutral edge path and hit-test primitives.
  - Added `PathCommand`, `EdgePath`, `EdgePathLabel`, `BezierEdgeOptions`, `SmoothStepEdgeOptions`, and `EdgeHitTestOptions`.
  - Added straight, bezier, and smoothstep-like path builders that return path commands rather than SVG strings.
  - Added numeric path distance and point containment hit testing with configurable interaction width and curve samples.
  - Added `NodeGraphInteractionState::edge_hit_test_options` to connect existing interaction config fields to geometry hit testing.
  - `cargo fmt --check`: passed.
  - `cargo nextest run -p jellyflow-runtime edge_path`: passed, 4 tests.
  - `cargo nextest run -p jellyflow-runtime hit_test`: passed, 3 tests.
  - `cargo check -p jellyflow-runtime`: passed.
  - Review: self-review against `review-workstream` checklist found no blocking or important findings for JGS-050.
- 2026-06-01: JGS-060 documented the shipped geometry surface.
  - Updated `README.md` and `crates/jellyflow-runtime/README.md` to mention renderer-neutral geometry.
  - Added `crates/jellyflow-runtime/examples/geometry_edge.rs`.
  - `cargo fmt --check`: passed.
  - `cargo nextest run -p jellyflow-runtime`: passed, 126 tests.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo run -p jellyflow-runtime --example geometry_edge`: passed and printed an edge label summary.
  - Review: self-review against `review-workstream` checklist found no blocking or important findings for JGS-060.
- 2026-06-01: JGS-070 closed the workstream after full verification.
  - Closeout claim: Jellyflow now has a headless runtime geometry/spatial substrate covering shared bounds/viewport math, deterministic lookup-derived spatial queries, endpoint geometry, edge path commands, and numeric edge hit testing.
  - `cargo fmt --check`: passed.
  - `cargo nextest run --workspace`: passed, 207 tests.
  - `cargo clippy --workspace --all-targets -- -D warnings`: passed.
  - `python3 tools/check_no_fret_dependencies.py`: passed.
  - `python3 tools/check_external_consumer_smoke.py`: passed.
  - `jq empty docs/workstreams/jellyflow-geometry-spatial-v1/WORKSTREAM.json`: passed after closeout edits.
  - `git diff --check`: passed after closeout edits.
  - Review: `review-workstream` self-review found no blocking findings.
  - Verification: `verify-rust-workstream` closeout gates passed with fresh command evidence.

## Notes

- Fresh verification is required before marking a task, Codex goal, or lane complete.
- Do not treat this lane as permission to move persisted layout or policy fields out of `Graph`.
- Do not port DOM or d3 code. Use XyFlow as a behavior reference for headless math only.
