# Jellyflow Node Drag Module Split v1 - Evidence And Gates

Status: Closed
Last updated: 2026-06-01

## Smallest Current Repro

`crates/jellyflow-runtime/src/runtime/drag.rs` is a 401-line module that mixes public API types,
planning, candidate filtering, snap/grid and extent constraints, and store extension methods.

## Required Gates

- `cargo fmt --check`
- `cargo nextest run -p jellyflow-runtime drag`
- `cargo nextest run -p jellyflow-runtime conformance`
- `cargo nextest run -p jellyflow-runtime --test public_surface`
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
- `jq empty docs/workstreams/jellyflow-node-drag-module-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-drag-module-split-v1/TASKS.jsonl docs/workstreams/jellyflow-node-drag-module-split-v1/CAMPAIGNS.jsonl`
- `git diff --check`

## Evidence Log

- 2026-06-01: JNDMS-010 opened the node drag module split lane.
  - Scope is a behavior-preserving module split under `jellyflow-runtime`.
  - Public API paths, node drag transaction behavior, snap/extent behavior, and renderer-free
    boundary are unchanged.
- 2026-06-01: JNDMS-020 split `runtime::drag` into focused submodules.
  - `crates/jellyflow-runtime/src/runtime/drag/mod.rs` is the facade.
  - `types.rs` owns public request, item, plan, and transaction-label types.
  - `planner.rs` owns pure transaction planning.
  - `candidates.rs` owns selected-node discovery and drag eligibility.
  - `constraints.rs` owns snap-grid, group bounds, extent, clamp, and size normalization.
  - `store.rs` owns `NodeGraphStore` node-drag extension methods.
  - `cargo check -p jellyflow-runtime --all-targets`: pass.
  - `cargo fmt --check`: pass.
  - `cargo nextest run -p jellyflow-runtime drag`: pass, 10 tests, run ID
    `1c45675f-f212-421d-b531-7de27a4fcd3f`.
  - `cargo nextest run -p jellyflow-runtime conformance`: pass, 26 tests, run ID
    `7170f849-3022-4f62-824e-6c16ed3bb4fc`.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: pass, 3 tests, run ID
    `8878791d-3627-4a7a-9706-582ffe4ec5d9`.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: pass.
  - `cargo nextest run -p jellyflow-runtime`: pass, 177 tests, run ID
    `136793b7-4b55-4bc3-939e-b6124564f90c`.
- 2026-06-01: JNDMS-030 closed the lane after review and verification.
  - `jq empty docs/workstreams/jellyflow-node-drag-module-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-drag-module-split-v1/TASKS.jsonl docs/workstreams/jellyflow-node-drag-module-split-v1/CAMPAIGNS.jsonl`: pass.
  - `git diff --check`: pass.
  - Review result: pass. The split is mechanical and behavior-preserving, with no public API drift
    and no renderer, adapter, parent expansion, or resize scope creep.

## Notes

Keep parent expansion, resize, pointer capture, adapter, and renderer behavior out of this lane.
