# Jellyflow Node Drag Parent Expansion v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

The workstream is active. It was opened as a follow-on to the closed node drag kernel and node drag
module split lanes.

JNPE-010 is complete: the lane scope, non-goals, source coverage, task ledger, campaign record,
milestones, gate set, context manifest, and machine-readable workstream metadata are recorded.

JNPE-020 is complete: `runtime::drag` now plans single-parent expansion by appending deterministic
`SetGroupRect` operations after `SetNodePos` operations when a child with effective
`expand_parent = true` would exceed the current parent group rect. `expand_parent = false` still
clamps `NodeExtent::Parent` movement to the current parent rect.

The remaining architecture gap is multi-node and left/top expansion behavior. XyFlow's drag update
path calls `handleExpandParent`, which expands parent rects and compensates non-dragged siblings
when the parent expands left or upward. Jellyflow now has the first `SetGroupRect` planning seam,
but JNPE-030 still needs to prove deterministic multi-parent ordering and decide whether absolute
group/node coordinates require sibling compensation.

## Next Task

JNPE-030: make parent expansion deterministic for multi-node drags, multiple parent groups, and
non-dragged sibling compensation when parent rects expand left or upward.

## Decisions Since Opening

- Keep parent expansion inside `runtime::drag`; adapters should not calculate group resizing.
- Reuse `GraphOp::SetGroupRect` for parent group expansion.
- Preserve current parent extent clamping when `expand_parent = false`.
- Keep resize handles, raw pointer capture, renderer smoke, and schema migration outside this lane.
- Treat nested parent cascading as a follow-on unless implementation evidence proves it is required.
- XyFlow keyboard movement uses the same `updateNodePositions` path, so Jellyflow's shared
  move-planner behavior may also expand parents during keyboard nudge.

## Blockers

- None known.

## Validation To Run

For JNPE-030:

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
- 2026-06-02: JNPE-020 added minimal single-parent expansion planning and focused runtime tests.

## Next Recommended Action

Start JNPE-030 with focused runtime tests in `crates/jellyflow-runtime/src/runtime/tests/drag`,
asserting that:

- multiple expanding parent groups produce deterministic `SetGroupRect` ordering;
- multi-selection expands each affected parent based on its moved children;
- left/top expansion either preserves Jellyflow's absolute sibling positions without compensation or
  adds explicit sibling `SetNodePos` compensation if the coordinate contract requires it.
