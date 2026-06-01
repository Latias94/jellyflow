# Jellyflow Node Drag Module Split v1 - Handoff

Status: Closed
Last updated: 2026-06-01

## Current State

The workstream is closed. `runtime::drag` was split from one broad module into a facade plus
focused `types`, `planner`, `candidates`, `constraints`, and `store` submodules.

JNDMS-010, JNDMS-020, and JNDMS-030 are complete. Public drag paths, store extension methods, node
drag transaction behavior, snap-grid behavior, extent clamping, conformance fixture behavior, and
the renderer-free runtime boundary remain unchanged.

## Next Task

None in this workstream.

## Decisions Since Opening

- Preserve `jellyflow_runtime::runtime::drag::*` public API paths.
- Keep node drag transaction behavior, deterministic ordering, snap, extent, and store dispatch
  semantics unchanged.
- Keep parent expansion, resize, pointer capture, adapter, and renderer behavior out of scope.

## Validation To Run

- Already run:
  - `cargo fmt --check`
  - `cargo nextest run -p jellyflow-runtime drag`
  - `cargo nextest run -p jellyflow-runtime conformance`
  - `cargo nextest run -p jellyflow-runtime --test public_surface`
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
  - `cargo nextest run -p jellyflow-runtime`
  - `jq empty docs/workstreams/jellyflow-node-drag-module-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-drag-module-split-v1/TASKS.jsonl docs/workstreams/jellyflow-node-drag-module-split-v1/CAMPAIGNS.jsonl`
  - `git diff --check`

## Evidence So Far

- 2026-06-01: JNDMS-010 opened the workstream.
- 2026-06-01: JNDMS-020 split `runtime::drag` into facade and owned submodules.
- 2026-06-01: JNDMS-030 recorded evidence and closed the workstream.

## Follow-On Candidates

- None required for this lane.
- Parent expansion, resize, pointer capture, adapter behavior, and renderer integration remain
  separate future scopes if they become product priorities.
