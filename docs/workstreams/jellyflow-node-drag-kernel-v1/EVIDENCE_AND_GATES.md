# Jellyflow Node Drag Kernel v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-01

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime adapter_conformance
```

This currently exercises the interaction harness and connect gesture contract. JND-020 should add a
drag-focused test filter.

## Gate Set

### Drag Kernel Gate

```bash
cargo nextest run -p jellyflow-runtime drag
cargo check -p jellyflow-runtime
```

This proves the headless drag kernel behavior without renderer dependencies.

### Adapter Conformance Gate

```bash
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo check -p jellyflow-runtime
```

This proves drag behavior integrates with the runtime harness and XyFlow compatibility projection.

### Runtime Package Gate

```bash
cargo nextest run -p jellyflow-runtime
```

This proves drag changes do not regress runtime behavior.

### Broader Closeout Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-node-drag-kernel-v1/WORKSTREAM.json
git diff --check
```

This proves formatting, runtime behavior, lint cleanliness, JSON validity, and diff hygiene.

## Evidence Anchors

- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `docs/adr/0002-jellyflow-model-policy-boundary.md`
- `docs/workstreams/jellyflow-interaction-harness-v1/HANDOFF.md`
- `docs/workstreams/jellyflow-geometry-spatial-v1/HANDOFF.md`
- `repo-ref/xyflow/packages/system/src/xydrag/XYDrag.ts`
- `repo-ref/xyflow/packages/system/src/xydrag/utils.ts`
- `crates/jellyflow-core/src/core/model/node.rs`
- `crates/jellyflow-core/src/ops/transaction/op.rs`
- `crates/jellyflow-runtime/src/runtime/policy/node.rs`

## Fresh Evidence Log

- 2026-06-01: JND-010 opened the node drag kernel workstream.
  - `git status --short --branch`: clean before opening docs, branch ahead of origin.
  - Governing decisions: ADR 0003 keeps drag fixtures renderer-free; ADR 0002 keeps persisted
    layout/policy fields in `Graph` for v1.
  - XyFlow reference reviewed: `xydrag/XYDrag.ts` and `xydrag/utils.ts`.
- 2026-06-01: JND-020 added the first single-node drag kernel slice.
  - Added public `runtime::drag` with `NodeDragRequest`, `NodeDragPlan`,
    `NODE_DRAG_TRANSACTION_LABEL`, `plan_node_drag`, and `NodeGraphStore::apply_node_drag`.
  - Added harness-backed drag fixtures for a committed `SetNodePos` trace and no-commit behavior.
  - Fixture coverage: per-node draggable policy, global `nodes_draggable`, hidden-node exclusion,
    missing node, no-op target, non-finite target, deterministic transaction label/op, and
    committed graph state.
  - `review-workstream` self-review: no blocking findings; continuous drag preview/final-commit
    semantics remain a named follow-on risk.
  - RED gate: `cargo nextest run -p jellyflow-runtime drag` failed before `runtime::drag` and
    `NodeGraphStore::apply_node_drag` existed.
  - `cargo fmt`: applied formatting after `cargo fmt --check` reported style-only differences.
  - `cargo fmt --check`: passed after formatting.
  - `cargo nextest run -p jellyflow-runtime drag`: passed, 4 tests.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 2 tests.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo nextest run -p jellyflow-runtime`: passed, 145 tests.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
- 2026-06-01: JND-030 added deterministic multi-selection drag planning.
  - Extended `plan_node_drag` and `NodeGraphStore::plan_node_drag` to use current view-state
    selection.
  - Added public `NodeDragItem` output and `NodeDragPlan::items`.
  - Multi-drag candidates are selected nodes plus the primary node, sorted by `NodeId`, filtered by
    existing draggable/hidden policy, and filtered when a node's parent group is selected.
  - Fixture coverage: selected-node co-dragging, primary node inclusion when not selected, sorted
    transaction ops, non-draggable selected node exclusion, and selected-parent child filtering via
    Jellyflow's selected group parent semantics.
  - RED gate: `cargo nextest run -p jellyflow-runtime multi_selection_drag` failed before
    `NodeDragItem`, `NodeDragPlan::items`, and view-state-aware planning existed.
  - `cargo fmt --check`: passed.
  - `cargo nextest run -p jellyflow-runtime multi_selection_drag`: passed, 1 test.
  - `cargo nextest run -p jellyflow-runtime drag`: passed, 5 tests.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 2 tests.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo nextest run -p jellyflow-runtime`: passed, 146 tests.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `review-workstream` self-review: no blocking findings; selected-parent filtering is documented
    as selected group parent semantics for Jellyflow.
- 2026-06-01: JND-040 added snap-to-grid and movement extent handling.
  - Added shared snap-offset planning for multi-selection drag based on the first deterministic drag
    item, matching the XyFlow reference behavior that keeps a selection moving as one group.
  - Added node-origin-aware bounds calculation for clamping node movement to extents.
  - Added global `node_extent` handling that clamps multi-selection movement as a group instead of
    letting individual nodes split apart at the boundary.
  - Added per-node rect extent handling and basic `NodeExtent::Parent` resolution to a parent group
    rect when `expand_parent` is false.
  - Fixture coverage: shared snap offset for multi-drag, global node extent, per-node rect extent,
    node origin, sorted drag items, and deterministic clamped targets.
  - RED gate: `cargo nextest run -p jellyflow-runtime multi_selection_drag_uses_shared_snap_offset`
    failed before snap handling because the primary target remained unsnapped.
  - RED gate:
    `cargo nextest run -p jellyflow-runtime multi_selection_drag_clamps_global_extent_as_group`
    failed before extent handling because multi-selection targets ignored global extent.
  - RED gate:
    `cargo nextest run -p jellyflow-runtime single_node_drag_clamps_per_node_rect_with_node_origin`
    failed before per-node extent handling because the target ignored node-origin-aware bounds.
  - `cargo fmt`: applied formatting after implementation.
  - `cargo nextest run -p jellyflow-runtime multi_selection_drag_uses_shared_snap_offset`: passed,
    1 test.
  - `cargo nextest run -p jellyflow-runtime multi_selection_drag_clamps_global_extent_as_group`:
    passed, 1 test.
  - `cargo nextest run -p jellyflow-runtime single_node_drag_clamps_per_node_rect_with_node_origin`:
    passed, 1 test.
  - `cargo nextest run -p jellyflow-runtime multi_selection_drag`: passed, 3 tests.
  - `cargo nextest run -p jellyflow-runtime drag`: passed, 8 tests.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo fmt --check`: passed.
  - `jq empty docs/workstreams/jellyflow-node-drag-kernel-v1/WORKSTREAM.json`: passed.
  - `git diff --check`: passed.
  - `cargo nextest run -p jellyflow-runtime`: passed, 149 tests.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `review-workstream` self-review: no blocking findings; residual risk is limited to parent
    expansion and untested mixed custom/global extent edge cases, both outside the JND-040 evidence
    contract.
- 2026-06-01: JND-050 added node drag gesture traces and XyFlow callback projection.
  - Added renderer-neutral `NodeDragStart`, `NodeDragUpdate`, `NodeDragEnd`, and
    `NodeDragEndOutcome` gesture payloads in `runtime::events`.
  - Split gesture event ownership into focused `connection`, `node_drag`, and `gesture` event
    modules.
  - Wired `NodeGraphGestureEvent::NodeDragStart`, `NodeGraphGestureEvent::NodeDragUpdate`, and
    `NodeGraphGestureEvent::NodeDragEnd` through `install_callbacks`.
  - Changed XyFlow-compatible `on_node_drag` to receive a `NodeDragUpdate` payload, so callback
    consumers get the same primary node, dragged nodes, and pointer intent as the raw gesture trace.
  - Extended the interaction harness callback recorder with node drag start/update/end events.
  - Fixture coverage: pointer intent, committed drag transaction, `NodeChange::Position`
    projection, gesture ordering, graph commit callback ordering, and node drag callback payloads.
  - RED gate:
    `cargo nextest run -p jellyflow-runtime adapter_conformance_harness_records_node_drag_gesture_transaction_and_callbacks`
    failed before node drag events, callback variants, and dispatch support existed.
  - `cargo fmt`: applied formatting after implementation.
  - `cargo nextest run -p jellyflow-runtime adapter_conformance_harness_records_node_drag_gesture_transaction_and_callbacks`:
    passed, 1 test.
  - `cargo nextest run -p jellyflow-runtime adapter_conformance`: passed, 8 tests.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 2 tests.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo fmt --check`: passed.
  - `jq empty docs/workstreams/jellyflow-node-drag-kernel-v1/WORKSTREAM.json`: passed.
  - `git diff --check`: passed.
  - `cargo nextest run -p jellyflow-runtime`: passed, 150 tests.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `review-workstream` self-review: no blocking findings; residual risk is that drag gesture
    start/update/end remains adapter-emitted rather than managed by a runtime drag session helper.

## Notes

- Fresh verification is required before marking a task, Codex goal, or lane complete.
- Keep renderer input capture, screenshots, and pixel tests outside `jellyflow-runtime`.
- Do not treat this lane as permission to move persisted node layout or policy fields out of
  `Graph`.
