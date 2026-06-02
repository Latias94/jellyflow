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

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
