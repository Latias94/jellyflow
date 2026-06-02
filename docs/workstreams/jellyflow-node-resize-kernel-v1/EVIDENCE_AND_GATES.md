# Jellyflow Node Resize Kernel v1 - Evidence And Gates

Status: Closed
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

### 2026-06-02 - JNR-030 Direction And Origin Resize Planning

Scope: `crates/jellyflow-runtime/src/runtime/resize`, `crates/jellyflow-runtime/src/runtime/tests/resize.rs`,
`crates/jellyflow-runtime/tests/public_surface.rs`

Result:

- Added XyFlow-style `NodeResizeDirection` vocabulary for top/right/bottom/left/corner controls.
- Added `NodeResizeContext` and a context-aware planner so store planning can use the resolved
  `node_origin` fallback.
- Planned left/top controls as position-before-size transactions using existing `SetNodePos` and
  `SetNodeSize` ops.
- Preserved graph-only `plan_node_resize` with the default `(0.0, 0.0)` origin.
- Added focused tests for left-edge position updates, global origin fallback, and per-node origin
  override behavior.

Split decision:

- Parent/child extent and keep-aspect-ratio parity remain split out of this task. XyFlow's
  `getDimensionsAfterResize` resolves them from pointer start values, pointer deltas, extent clamp
  distances, and child extents. Jellyflow's current resize request is a target-size request; adding
  exact extent parity here would either invent a pointer lifecycle or silently misrepresent clamp
  behavior. The next stable step is conformance/template coverage for the current target-size
  planner, then a dedicated pointer-resize session request if adapter evidence needs exact
  `XYResizer` parity.

Behavior proven:

- `Left` resize emits `SetNodePos` before `SetNodeSize`.
- Store resize planning uses global `node_origin` fallback.
- Per-node `Node.origin` overrides the global fallback.
- Drag parent expansion tests still pass after adding resize position planning.
- Public surface exposes direction and context-aware resize planner vocabulary.

Fresh verification:

- 2026-06-02: `cargo fmt --check` passed.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime resize` passed, 6 tests run.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime drag_parent_expansion` passed, 6 tests run.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime --test public_surface` passed, 3 tests run.
- 2026-06-02: `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings` passed.

### 2026-06-02 - JNR-040 Conformance And Template Resize Coverage

Scope: `crates/jellyflow-runtime/src/runtime/conformance`, `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance`,
`templates/headless-adapter`

Result:

- Added `apply_node_resize` conformance action vocabulary with serde-friendly request, constraint,
  and direction wrappers.
- Wired the conformance runner to `NodeGraphStore::apply_node_resize`.
- Added runner coverage for size-only resize traces.
- Added adapter conformance coverage for direction-aware resize traces that emit `set_node_pos`
  before `set_node_size`.
- Added a template node resize scenario to the built-in headless adapter suite and fixture directory
  roundtrip tests.

Behavior proven:

- Fixture authors can represent target-size node resize through the high-level conformance action.
- Runtime conformance trace records `node resize` / `set_node_size` for size-only resize.
- Adapter conformance trace records `node resize` / `set_node_pos`, `set_node_size` for left-edge
  resize.
- Template `check` includes `template node resize` and still matches expected traces.

Fresh verification:

- 2026-06-02: `cargo fmt --check` passed.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime conformance` passed, 55 tests run.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime adapter_conformance` passed, 18 tests run.
- 2026-06-02: `cargo test --manifest-path templates/headless-adapter/Cargo.toml` passed, 9 tests run.
- 2026-06-02: `cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check` passed and reported matching built-in suite traces.
- 2026-06-02: `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings` passed.

### 2026-06-02 - JNR-050 Documentation And Closeout

Scope: `README.md`, `crates/jellyflow-runtime/README.md`, `CONTEXT.md`,
`docs/workstreams/jellyflow-node-resize-kernel-v1`

Result:

- Documented `runtime::resize` in the root and runtime README interaction contracts.
- Documented the runtime/adapter split for target-size resize planning versus pointer capture,
  resize handles, renderer feedback, screenshots, and pixels.
- Kept exact pointer-resize parent/child extent and keep-aspect-ratio parity split until adapter
  evidence needs a pointer-session request.
- Closed workstream metadata, task ledger, campaign record, handoff, and closeout audit.

Behavior proven:

- All runtime resize, direction/origin, conformance, adapter conformance, and template behavior
  remains covered by the package closeout gate.
- Headless crates still avoid renderer, DOM, d3, wgpu, egui, Fret, screenshot, and pixel
  dependencies.

Fresh verification:

- 2026-06-02: `cargo fmt --check` passed.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime` passed, 291 tests run.
- 2026-06-02: `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings` passed.
- 2026-06-02: `jq empty docs/workstreams/jellyflow-node-resize-kernel-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-resize-kernel-v1/TASKS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl` passed.
- 2026-06-02: `git diff --check` passed.

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
