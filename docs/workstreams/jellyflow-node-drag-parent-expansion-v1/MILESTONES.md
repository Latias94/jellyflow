# Jellyflow Node Drag Parent Expansion v1 - Milestones

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

Exit criteria:

- Problem and target state are explicit.
- XyFlow `expandParent` drag and helper paths are linked.
- Non-goals keep renderer, resize handles, pointer capture, and schema migration outside this lane.
- First executable task is `JNPE-020`.

Status: complete on 2026-06-02.

## M1 - Single-Parent Drag Expansion

Exit criteria:

- A child node with `NodeExtent::Parent` and `expand_parent = true` can expand its parent group
  during drag.
- The resulting transaction includes deterministic `SetNodePos` and `SetGroupRect` operations.
- `expand_parent = false` keeps the current parent clamp behavior.
- No renderer or adapter dependency is added.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime drag_parent_expansion
cargo nextest run -p jellyflow-runtime drag
```

Status: pending.

## M2 - Multi-Selection And Sibling Compensation

Exit criteria:

- Multi-node drags expand parent groups deterministically.
- Multiple parent groups are ordered deterministically in the planned transaction.
- Non-dragged sibling compensation for parent left/top expansion is implemented or explicitly split
  with evidence.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime drag_parent_expansion
cargo nextest run -p jellyflow-runtime drag
```

Status: pending.

## M3 - Conformance And Adapter Trace Coverage

Exit criteria:

- Adapter-facing conformance coverage proves parent expansion transaction behavior, or the lane
  records why existing transaction dispatch coverage is enough.
- Template smoke coverage is updated when fixture vocabulary changes.
- Public surface smoke is updated if new exported types or actions are added.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

Status: pending.

## M4 - Documentation And Closeout

Exit criteria:

- README/runtime README explain the runtime-owned parent expansion planner and adapter-owned raw
  input/rendering boundary.
- Workstream evidence is current and machine-readable state is valid.
- Remaining resize, nested-cascade, or renderer smoke follow-ons are split or deferred.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-node-drag-parent-expansion-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-drag-parent-expansion-v1/TASKS.jsonl docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl
git diff --check
```

Status: pending.
