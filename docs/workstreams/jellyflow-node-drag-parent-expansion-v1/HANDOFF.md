# Jellyflow Node Drag Parent Expansion v1 - Handoff

Status: Closed
Last updated: 2026-06-02

## Current State

The workstream is closed. It was opened as a follow-on to the closed node drag kernel and node drag
module split lanes.

JNPE-010 is complete: the lane scope, non-goals, source coverage, task ledger, campaign record,
milestones, gate set, context manifest, and machine-readable workstream metadata are recorded.

JNPE-020 is complete: `runtime::drag` now plans single-parent expansion by appending deterministic
`SetGroupRect` operations after `SetNodePos` operations when a child with effective
`expand_parent = true` would exceed the current parent group rect. `expand_parent = false` still
clamps `NodeExtent::Parent` movement to the current parent rect.

JNPE-030 is complete: multi-parent expansion order is tested and deterministic. Left/top expansion
updates the parent group rect while preserving non-dragged sibling node positions. This is
intentional because Jellyflow stores node positions in canvas space, unlike XyFlow's
parent-relative child positions that require sibling compensation when parent position changes.

JNPE-040 is complete: conformance and template coverage reuse the existing `ApplyNodeDrag` action,
so no fixture schema change was needed. Runtime conformance, adapter conformance, and the external
headless adapter template now cover parent expansion traces with `set_node_pos` and
`set_group_rect`.

JNPE-050 is complete: README/runtime README document runtime-owned parent expansion planning versus
adapter-owned raw input, resize handles, renderer smoke, screenshots, and pixels. `CONTEXT.md`
again reflects that all current workstreams are closed.

## Next Task

None in this workstream. Follow-ons are split below.

## Decisions Since Opening

- Keep parent expansion inside `runtime::drag`; adapters should not calculate group resizing.
- Reuse `GraphOp::SetGroupRect` for parent group expansion.
- Preserve current parent extent clamping when `expand_parent = false`.
- Keep resize handles, raw pointer capture, renderer smoke, and schema migration outside this lane.
- Treat nested parent cascading as a follow-on unless implementation evidence proves it is required.
- XyFlow keyboard movement uses the same `updateNodePositions` path, so Jellyflow's shared
  move-planner behavior may also expand parents during keyboard nudge.
- Do not add XyFlow-style sibling compensation ops for left/top parent expansion while Jellyflow
  keeps node positions in canvas space.
- Do not add a parent-expansion fixture action while `ApplyNodeDrag` covers the interaction
  boundary.

## Blockers

- None known.

## Validation To Run

Already run for closeout:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-node-drag-parent-expansion-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-drag-parent-expansion-v1/TASKS.jsonl docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl
git diff --check
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
- 2026-06-02: JNPE-030 proved multi-parent ordering and left/top no-compensation coordinate
  behavior with focused runtime tests.
- 2026-06-02: JNPE-040 added conformance/template parent expansion traces without changing fixture
  schema.
- 2026-06-02: JNPE-050 documented closeout boundaries and closed the workstream.

## Closeout Evidence

- 2026-06-02: `cargo fmt --check` passed.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime` passed, 283 tests run.
- 2026-06-02: `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings` passed.
- 2026-06-02: `jq empty docs/workstreams/jellyflow-node-drag-parent-expansion-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-drag-parent-expansion-v1/TASKS.jsonl docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl`
  passed.
- 2026-06-02: `git diff --check` passed.

## Next Recommended Action

- Node resize parent expansion, nested parent cascading, or parent-relative coordinate semantics
  only if adapter integration proves the v1 canvas-space drag expansion contract is insufficient.
- Renderer smoke, screenshot, or pixel tests belong in future wgpu, egui, Fret, or other adapter
  crates outside `jellyflow-runtime`.
