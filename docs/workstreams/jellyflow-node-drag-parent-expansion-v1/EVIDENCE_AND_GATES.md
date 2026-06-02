# Jellyflow Node Drag Parent Expansion v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-02

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime drag_parent_expansion
```

JNPE-020 should add the first focused repro. Until then, use the metadata gate to verify the lane
itself.

## Gate Set

### Single-Parent Expansion Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime drag_parent_expansion
cargo nextest run -p jellyflow-runtime drag
```

### Multi-Selection And Sibling Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime drag_parent_expansion
cargo nextest run -p jellyflow-runtime drag
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
jq empty docs/workstreams/jellyflow-node-drag-parent-expansion-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-drag-parent-expansion-v1/TASKS.jsonl docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl
git diff --check
```

## Evidence Anchors

- `docs/workstreams/jellyflow-node-drag-parent-expansion-v1/DESIGN.md`
- `docs/workstreams/jellyflow-node-drag-parent-expansion-v1/TODO.md`
- `docs/workstreams/jellyflow-node-drag-parent-expansion-v1/TASKS.jsonl`
- `docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CAMPAIGNS.jsonl`
- `crates/jellyflow-runtime/src/runtime/drag/constraints/extent.rs`
- `crates/jellyflow-runtime/src/runtime/drag/planner.rs`
- `crates/jellyflow-runtime/src/runtime/tests/drag`
- `repo-ref/xyflow/packages/react/src/store/index.ts`
- `repo-ref/xyflow/packages/system/src/utils/store.ts`
- `repo-ref/xyflow/packages/system/src/utils/graph.ts`

## Evidence Log

### 2026-06-02 - JNPE-010 Workstream Opened

Scope: `docs/workstreams/jellyflow-node-drag-parent-expansion-v1`, `CONTEXT.md`

Result:

- Opened the node drag parent expansion lane from closed node drag follow-ons.
- Set `JNPE-020` as the first executable task.
- Recorded XyFlow source evidence for `updateNodePositions`, `handleExpandParent`, and parent
  extent clamping.
- Recorded the current Jellyflow shallow seam: `expand_parent = true` currently removes parent
  clamping but does not plan `SetGroupRect`.

Behavior proven:

- Planning artifacts agree on target state, task order, gates, source coverage, and autonomous
  commit policy.

Fresh verification:

- Pending for the opening commit.

### 2026-06-02 - JNPE-020 Single-Parent Drag Expansion

Scope:

- `crates/jellyflow-runtime/src/runtime/drag/candidates.rs`
- `crates/jellyflow-runtime/src/runtime/drag/constraints/geometry.rs`
- `crates/jellyflow-runtime/src/runtime/drag/constraints/mod.rs`
- `crates/jellyflow-runtime/src/runtime/drag/mod.rs`
- `crates/jellyflow-runtime/src/runtime/drag/parent_expansion.rs`
- `crates/jellyflow-runtime/src/runtime/drag/planner.rs`
- `crates/jellyflow-runtime/src/runtime/tests/drag/parent_expansion.rs`
- `crates/jellyflow-runtime/src/runtime/tests/drag/mod.rs`

Result:

- Added a private `runtime::drag::parent_expansion` planner helper.
- Drag candidates now carry parent group id and effective `expand_parent` policy.
- Node move planning appends deterministic `SetGroupRect` operations after `SetNodePos` operations
  when an expanding child would exceed the current parent group rect.
- `expand_parent = false` continues to clamp `NodeExtent::Parent` movement to the current parent
  group rect.

Behavior proven:

- A child with `NodeExtent::Parent` and `expand_parent = false` clamps to the current parent rect and
  emits only `SetNodePos`.
- A child with `NodeExtent::Parent` and `expand_parent = true` can move past the current parent edge
  and emits `SetNodePos` followed by `SetGroupRect`.
- Applying the planned drag updates both child position and parent group rect through normal
  `NodeGraphStore` graph commit events.
- Existing drag, nudge, gesture, conformance, and adapter drag tests still pass.

Fresh verification:

- Red check: `cargo nextest run -p jellyflow-runtime drag_parent_expansion` failed before
  implementation because the enabled expansion plan emitted only `SetNodePos`.
- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime drag_parent_expansion`: passed, 2 tests run, 2 passed.
- `cargo nextest run -p jellyflow-runtime drag`: passed, 40 tests run, 40 passed.
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.

### 2026-06-02 - JNPE-030 Multi-Parent And Left/Top Expansion Contract

Scope:

- `crates/jellyflow-runtime/src/runtime/tests/drag/parent_expansion.rs`
- `docs/workstreams/jellyflow-node-drag-parent-expansion-v1/DESIGN.md`
- `docs/workstreams/jellyflow-node-drag-parent-expansion-v1/EVIDENCE_AND_GATES.md`
- `docs/workstreams/jellyflow-node-drag-parent-expansion-v1/HANDOFF.md`

Result:

- Added runtime tests proving deterministic multi-parent expansion ordering.
- Added runtime tests proving left/top expansion updates the parent group rect while preserving
  non-dragged sibling node positions.
- Recorded Jellyflow's coordinate decision: node positions are canvas-space absolute, so XyFlow's
  parent-relative sibling compensation does not become extra `SetNodePos` ops in this runtime
  contract.

Behavior proven:

- Multi-selection moves selected expanding children with sorted node ops and sorted parent group
  expansion ops.
- Multiple parent groups expand based on moved children in deterministic `GroupId` order.
- Left/top expansion changes the group rect origin and size without moving non-dragged siblings.
- Existing drag, nudge, gesture, conformance, and adapter drag tests still pass.

Fresh verification:

- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime drag_parent_expansion`: passed, 4 tests run, 4 passed.
- `cargo nextest run -p jellyflow-runtime drag`: passed, 42 tests run, 42 passed.

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
