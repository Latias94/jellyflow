# Jellyflow Node Resize Kernel v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-02

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime resize
```

JNR-020 should add the first focused runtime repro. Until then, use the metadata gate to verify the
lane itself.

## Gate Set

### Minimal Planner Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime resize
cargo nextest run -p jellyflow-runtime --test public_surface
```

### Origin And Extent Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime resize
cargo nextest run -p jellyflow-runtime drag_parent_expansion
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
jq empty docs/workstreams/jellyflow-node-resize-kernel-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-resize-kernel-v1/TASKS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl
git diff --check
```

## Evidence Anchors

- `docs/workstreams/jellyflow-node-resize-kernel-v1/DESIGN.md`
- `docs/workstreams/jellyflow-node-resize-kernel-v1/TODO.md`
- `docs/workstreams/jellyflow-node-resize-kernel-v1/TASKS.jsonl`
- `docs/workstreams/jellyflow-node-resize-kernel-v1/CAMPAIGNS.jsonl`
- `repo-ref/xyflow/packages/system/src/xyresizer/`
- `crates/jellyflow-core/src/ops/transaction/op.rs`
- `crates/jellyflow-runtime/src/runtime/xyflow/transaction/nodes.rs`
- `crates/jellyflow-runtime/src/runtime/xyflow/projection/node_graph/nodes.rs`

## Evidence Log

### 2026-06-02 - JNR-010 Workstream Opened

Scope: `docs/workstreams/jellyflow-node-resize-kernel-v1`, `CONTEXT.md`

Result:

- Opened the node resize kernel lane from XyFlow `XYResizer` source evidence.
- Set `JNR-020` as the first executable task.
- Recorded existing `Node.size`, `SetNodeSize`, `SetNodePos`, and `SetGroupRect` as early planning
  operations.
- Kept DOM handles, renderer UI, `ResizeObserver`, screenshots, and pixels outside runtime.

Behavior proven:

- Planning artifacts agree on target state, task order, gates, source coverage, and autonomous
  commit policy.

Fresh verification:

- 2026-06-02: `jq empty docs/workstreams/jellyflow-node-resize-kernel-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-resize-kernel-v1/TASKS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl` passed.
- 2026-06-02: `git diff --check` passed.

### 2026-06-02 - JNR-020 Minimal Pure Resize Planner

Scope: `crates/jellyflow-runtime/src/runtime/resize`, `crates/jellyflow-runtime/src/runtime/tests`,
`crates/jellyflow-runtime/tests/public_surface.rs`

Result:

- Added `runtime::resize` request, constraint, item, plan, planner, and store helper surface.
- Planned visible single-node resize requests as deterministic `SetNodeSize` transactions.
- Added optional min/max size clamping inside the runtime planner.
- Rejected hidden, missing, no-op, non-positive, non-finite, invalid-constraint, and contradictory
  constraint requests.
- Kept direction, origin, top/left position changes, extents, parent expansion, conformance schema,
  renderer UI, raw pointer capture, and pixels out of this slice.

Behavior proven:

- Accepted resize commits a `node resize` labeled `SetNodeSize` transaction and graph trace.
- Min/max constraints clamp target width/height deterministically.
- Invalid requests produce no plan and no store commit.
- Public surface exposes `runtime::resize` request/plan/constraint vocabulary.

Fresh verification:

- 2026-06-02: `cargo fmt --check` passed.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime resize` passed, 3 tests run.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime --test public_surface` passed, 3 tests run.
- 2026-06-02: `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings` passed.

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
