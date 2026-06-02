# Jellyflow Node Drag Parent Expansion v1 - Evidence And Gates

Status: Closed
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

### 2026-06-02 - JNPE-040 Conformance And Template Parent Expansion Traces

Scope:

- `crates/jellyflow-runtime/src/runtime/tests/conformance/runner/scenario.rs`
- `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/fixture_runner/mod.rs`
- `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/fixture_runner/node_drag.rs`
- `templates/headless-adapter/src/lib.rs`
- `templates/headless-adapter/tests/conformance.rs`

Result:

- Reused existing `ConformanceAction::ApplyNodeDrag`; no fixture schema change was needed.
- Added a runtime conformance runner scenario that records `set_node_pos` plus `set_group_rect`.
- Added an adapter conformance scenario for parent expansion transaction traces and callback counts.
- Added a fifth headless adapter template smoke scenario: `template node drag parent expansion`.

Behavior proven:

- Conformance fixtures can express parent expansion through the same interaction boundary adapters
  already call for node drag.
- XyFlow callback traces still report one node change and zero edge changes while the graph commit
  op kinds include `set_group_rect`.
- The external headless adapter template can save, load, and check a built-in suite containing a
  parent expansion scenario before renderer-specific smoke tests.

Fresh verification:

- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime conformance`: passed, 53 tests run, 53 passed.
- `cargo nextest run -p jellyflow-runtime adapter_conformance`: passed, 17 tests run, 17 passed.
- `cargo test --manifest-path templates/headless-adapter/Cargo.toml`: passed, 8 tests run, 8
  passed.
- `cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check`: passed, 5 matching
  scenarios.

### 2026-06-02 - JNPE-050 Documentation And Closeout

Scope:

- `README.md`
- `crates/jellyflow-runtime/README.md`
- `CONTEXT.md`
- `docs/workstreams/jellyflow-node-drag-parent-expansion-v1`

Result:

- README/runtime README now document parent expansion as a runtime-owned drag transaction planning
  contract.
- Documentation calls out adapter ownership of pointer capture, drag handles, resize handles,
  renderer grouping UI, screenshots, and pixels.
- `CONTEXT.md` no longer points to this workstream as active.
- Workstream state is closed with renderer, resize, nested-cascade, and parent-relative coordinate
  follow-ons deferred until adapter evidence requires them.

Fresh verification:

- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime`: passed, 283 tests run, 283 passed.
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
- `jq empty docs/workstreams/jellyflow-node-drag-parent-expansion-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-drag-parent-expansion-v1/TASKS.jsonl docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl`: passed.
- `git diff --check`: passed.

Review:

- Workstream compliance: JNPE-010 through JNPE-050 are complete, the target state is met, and ADR
  0001/0002/0003 boundaries remain intact.
- Code quality: parent expansion stays inside `runtime::drag`, uses existing reversible
  `GraphOp::SetGroupRect`, keeps public request shape unchanged, and is covered through runtime,
  conformance, adapter conformance, and template surfaces.
- Residual risks are split as follow-ons rather than kept in this closed lane.

Behavior proven:

- Node drag parent expansion can be planned, applied, conformance-replayed, and smoked through the
  external headless adapter template without renderer dependencies.
- Left/top group expansion follows Jellyflow's canvas-space coordinate contract and does not create
  XyFlow-style sibling compensation ops.

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
