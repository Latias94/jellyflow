# Jellyflow Visible Elements Contract v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-02

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime visible_node
```

JVE-020 should add the first focused runtime repro.

## Gate Set

### Visible Node Runtime Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime visible_node
cargo nextest run -p jellyflow-runtime --test public_surface
```

### Conformance And Template Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

### Package And Closeout Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
```

### Metadata And Diff Gate

```bash
jq empty docs/workstreams/jellyflow-visible-elements-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-visible-elements-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-visible-elements-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-visible-elements-contract-v1/CONTEXT.jsonl
git diff --check
```

## Evidence Anchors

- `docs/workstreams/jellyflow-visible-elements-contract-v1/DESIGN.md`
- `docs/workstreams/jellyflow-visible-elements-contract-v1/TODO.md`
- `docs/workstreams/jellyflow-visible-elements-contract-v1/TASKS.jsonl`
- `docs/workstreams/jellyflow-visible-elements-contract-v1/CAMPAIGNS.jsonl`
- `docs/workstreams/jellyflow-geometry-spatial-v1/HANDOFF.md`
- `repo-ref/xyflow/packages/react/src/hooks/useVisibleNodeIds.ts`
- `repo-ref/xyflow/packages/system/src/utils/graph.ts`
- `repo-ref/xyflow/packages/react/src/types/component-props.ts`
- `crates/jellyflow-runtime/src/runtime/utils/bounds.rs`
- `crates/jellyflow-runtime/src/runtime/viewport/transform.rs`
- `crates/jellyflow-runtime/src/io/config/state/views/rendering.rs`

## Evidence Log

### 2026-06-02 - JVE-010 Workstream Opened

Scope: `docs/workstreams/jellyflow-visible-elements-contract-v1`, `CONTEXT.md`

Result:

- Opened the visible elements contract lane from XyFlow `onlyRenderVisibleElements` source
  evidence and current Jellyflow geometry utilities.
- Set `JVE-020` as the first executable task.
- Kept real spatial indexing and visible edge culling outside the first contract.

Behavior proven:

- Planning artifacts agree on target state, task order, gates, source coverage, and autonomous
  commit policy.

Fresh verification:

- Passed 2026-06-02:
  - `jq empty docs/workstreams/jellyflow-visible-elements-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-visible-elements-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-visible-elements-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-visible-elements-contract-v1/CONTEXT.jsonl`
  - `git diff --check`

### 2026-06-02 - JVE-020 Visible Node Runtime Contract

Scope: `crates/jellyflow-runtime/src/runtime`, `crates/jellyflow-runtime/src/runtime/tests`,
`crates/jellyflow-runtime/tests/public_surface.rs`

Result:

- Added `runtime::rendering::VisibleNodeIdsRequest`.
- Added `runtime::rendering::resolve_visible_node_ids` using the existing deterministic
  `get_nodes_inside` linear scan.
- Added `NodeGraphStore::visible_node_ids(viewport_size)` using current view-state transform,
  resolved `only_render_visible_elements`, and node-origin fallback.
- Added runtime and public surface tests.

Behavior proven:

- culling enabled returns partially visible, non-hidden nodes for the current viewport;
- culling disabled returns all non-hidden node ids deterministically;
- invalid viewport size returns an empty list instead of panicking;
- store helper reads runtime tuning and node-origin config;
- unsized nodes enter culling only when a fallback size is supplied.

Fresh verification:

- Passed 2026-06-02: `cargo fmt --check`
- Passed 2026-06-02: `cargo nextest run -p jellyflow-runtime visible_node`
- Passed 2026-06-02: `cargo nextest run -p jellyflow-runtime --test public_surface`
- Passed 2026-06-02: `jq empty docs/workstreams/jellyflow-visible-elements-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-visible-elements-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-visible-elements-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-visible-elements-contract-v1/CONTEXT.jsonl`
- Passed 2026-06-02: `git diff --check`
- Deferred until JVE-040 closeout: full `cargo nextest run -p jellyflow-runtime` and
  `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`, because JVE-020's approved
  gate is the focused visible-node runtime contract plus public surface smoke.

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
