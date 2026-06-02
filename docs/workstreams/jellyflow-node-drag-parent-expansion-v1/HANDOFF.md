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

JNPE-030 is complete: multi-parent expansion order is tested and deterministic. Left/top expansion
updates the parent group rect while preserving non-dragged sibling node positions. This is
intentional because Jellyflow stores node positions in canvas space, unlike XyFlow's
parent-relative child positions that require sibling compensation when parent position changes.

## Next Task

JNPE-040: add conformance/template coverage for parent expansion transactions and any adapter-facing
node-change or callback traces affected by expanded parent groups, or record a no-schema-change
decision if existing transaction dispatch coverage is enough.

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

## Blockers

- None known.

## Validation To Run

For JNPE-040:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
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

## Next Recommended Action

Start JNPE-040 by checking the current conformance fixture vocabulary:

- if `DispatchTransaction` already proves `SetGroupRect` replay and callback traces, record a
  no-schema-change decision and only add adapter-facing tests where coverage is missing;
- otherwise add a dedicated parent-expansion conformance action or template scenario without
  renderer dependencies.
