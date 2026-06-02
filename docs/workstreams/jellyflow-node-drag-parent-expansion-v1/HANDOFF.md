# Jellyflow Node Drag Parent Expansion v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

The workstream is active. It was opened as a follow-on to the closed node drag kernel and node drag
module split lanes.

JNPE-010 is complete: the lane scope, non-goals, source coverage, task ledger, campaign record,
milestones, gate set, context manifest, and machine-readable workstream metadata are recorded.

The architecture gap is specific: current Jellyflow drag planning resolves `NodeExtent::Parent`
with `expand_parent = true` to no parent clamp, then emits only `SetNodePos` operations. XyFlow's
drag update path instead calls `handleExpandParent`, which expands parent rects and compensates
non-dragged siblings when the parent expands left or upward.

## Next Task

JNPE-020: implement the minimal runtime drag planner behavior for one dragged child expanding one
parent group while preserving `expand_parent = false` clamping.

## Decisions Since Opening

- Keep parent expansion inside `runtime::drag`; adapters should not calculate group resizing.
- Reuse `GraphOp::SetGroupRect` for parent group expansion.
- Preserve current parent extent clamping when `expand_parent = false`.
- Keep resize handles, raw pointer capture, renderer smoke, and schema migration outside this lane.
- Treat nested parent cascading as a follow-on unless implementation evidence proves it is required.

## Blockers

- None known.

## Validation To Run

For JNPE-020:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime drag_parent_expansion
cargo nextest run -p jellyflow-runtime drag
```

For lane closeout:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-node-drag-parent-expansion-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-drag-parent-expansion-v1/TASKS.jsonl docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl
git diff --check
```

## Evidence So Far

- 2026-06-02: JNPE-010 opened the workstream from XyFlow `expandParent` source evidence and current
  Jellyflow drag planner gaps.

## Next Recommended Action

Start JNPE-020 with red tests in `crates/jellyflow-runtime/src/runtime/tests/drag`, asserting that:

- `expand_parent = false` still clamps a child to the current parent group rect;
- `expand_parent = true` can move the child past the current parent edge;
- the planned transaction includes a deterministic `SetGroupRect` for the parent group.
