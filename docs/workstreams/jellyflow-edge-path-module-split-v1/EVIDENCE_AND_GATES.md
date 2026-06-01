# Jellyflow Edge Path Module Split v1 - Evidence And Gates

Status: Closed
Last updated: 2026-06-01

## Smallest Current Repro

`crates/jellyflow-runtime/src/runtime/geometry/paths.rs` is a single module mixing public path
types, straight path generation, bezier control math, smoothstep routing, label helpers, numeric
helpers, and tests.

## Required Gates

- `cargo fmt --check`
- `cargo nextest run -p jellyflow-runtime geometry::paths`
- `cargo nextest run -p jellyflow-runtime --test public_surface`
- `cargo nextest run -p jellyflow-runtime`
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
- `jq empty docs/workstreams/jellyflow-edge-path-module-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-edge-path-module-split-v1/TASKS.jsonl docs/workstreams/jellyflow-edge-path-module-split-v1/CAMPAIGNS.jsonl`
- `git diff --check`

## Evidence Log

- 2026-06-01: JEPM-010 opened the edge path module split lane.
  - Scope is a behavior-preserving private module split under `jellyflow-runtime`.
  - Public geometry paths, path command output, label placement, and renderer-free boundary must
    remain unchanged.
- 2026-06-01: JEPM-020 split `runtime::geometry::paths` into focused private submodules.
  - `paths/mod.rs` is the private facade.
  - `types.rs` owns public path command, label, and path types.
  - `straight.rs` owns straight path generation.
  - `bezier.rs` owns bezier options, path generation, and control-point helpers.
  - `smoothstep.rs` owns smoothstep-like options, orthogonal routing, and route helpers.
  - `label.rs` owns shared label placement helpers.
  - `tests.rs` owns existing path behavior tests.
  - `cargo check -p jellyflow-runtime --all-targets`: pass.
  - `cargo fmt --check`: pass.
  - `cargo nextest run -p jellyflow-runtime geometry::paths`: pass, 3 tests, run ID
    `92838e2a-d30b-4adb-975f-4f6ea45a39d0`.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: pass, 3 tests, run ID
    `67919f01-57ea-42a2-b086-ded53721f23e`.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: pass.
  - `cargo nextest run -p jellyflow-runtime`: pass, 177 tests, run ID
    `01fc49f3-7228-42ba-b7b7-0bf07f6452c8`.
- 2026-06-01: JEPM-030 closed the lane after review and verification.
  - `jq empty docs/workstreams/jellyflow-edge-path-module-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-edge-path-module-split-v1/TASKS.jsonl docs/workstreams/jellyflow-edge-path-module-split-v1/CAMPAIGNS.jsonl`: pass.
  - `git diff --check`: pass.
  - Review result: pass. The split is behavior-preserving and has no public API, edge command,
    label, routing, hit-test, adapter, renderer, or new routing-algorithm scope creep.

## Notes

Do not add new routing algorithms, adapter-specific path conversion, renderer code, platform code,
spatial-index code, or screenshot/pixel assets in this lane.
