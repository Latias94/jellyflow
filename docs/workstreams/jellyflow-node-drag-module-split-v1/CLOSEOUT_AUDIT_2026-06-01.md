# Jellyflow Node Drag Module Split v1 - Closeout Audit

Date: 2026-06-01
Status: Closed

## Result

`runtime::drag` was split from one broad module into:

- `mod.rs`: facade and public re-exports.
- `types.rs`: public request, item, plan, and transaction-label definitions.
- `planner.rs`: pure drag planning and transaction construction.
- `candidates.rs`: selected-node discovery and drag eligibility.
- `constraints.rs`: snap-grid, bounds, extent, clamp, and size normalization logic.
- `store.rs`: `NodeGraphStore` drag extension methods.

## Review

Review result: pass.

- Public `jellyflow_runtime::runtime::drag::*` paths are preserved.
- `NodeGraphStore::plan_node_drag` and `NodeGraphStore::apply_node_drag` remain available through
  the same crate module graph.
- Node drag operation ordering, transaction labeling, snap-grid behavior, extent clamping, and
  selected-node co-drag semantics are unchanged.
- The split did not add renderer, adapter, pointer capture, parent expansion, resize, or fixture
  schema behavior.

## Verification

- `cargo fmt --check`: pass.
- `cargo nextest run -p jellyflow-runtime drag`: pass, 10 tests.
- `cargo nextest run -p jellyflow-runtime conformance`: pass, 26 tests.
- `cargo nextest run -p jellyflow-runtime --test public_surface`: pass, 3 tests.
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: pass.
- `cargo nextest run -p jellyflow-runtime`: pass, 177 tests.
- `jq empty docs/workstreams/jellyflow-node-drag-module-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-drag-module-split-v1/TASKS.jsonl docs/workstreams/jellyflow-node-drag-module-split-v1/CAMPAIGNS.jsonl`: pass.
- `git diff --check`: pass.

## Follow-Ons

None required for this module-boundary lane. Parent expansion, resize, adapter behavior, and
renderer integration should remain separate future workstreams if they become priorities.

## Residual Risk

Low. This was a behavior-preserving split guarded by drag, conformance, public-surface, package, and
lint gates.
