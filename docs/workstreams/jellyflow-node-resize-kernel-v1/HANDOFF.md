# Jellyflow Node Resize Kernel v1 - Handoff

Status: Closed
Last updated: 2026-06-02

## Current State

The workstream is active. It was opened after the closed node drag parent expansion lane.

JNR-010 is complete: the lane scope, non-goals, source coverage, task ledger, campaign record,
milestones, gate set, context manifest, and machine-readable workstream metadata are recorded.

JNR-020 is complete: `runtime::resize` now exposes a renderer-neutral single-node resize request,
constraint, item, plan, pure planner, and `NodeGraphStore` helper. The first slice emits
deterministic `SetNodeSize` transactions, clamps optional min/max bounds, and rejects hidden,
missing, no-op, non-positive, non-finite, invalid-constraint, and contradictory-constraint requests.

JNR-030 is complete: resize requests now carry XyFlow-style control directions, plans expose
position movement, left/top controls emit `SetNodePos` before `SetNodeSize`, and store planning uses
resolved global node origin with per-node origin override support.

JNR-040 is complete: conformance fixtures now expose `apply_node_resize`, the runner applies it
through `NodeGraphStore`, adapter conformance covers direction-aware resize traces, and the headless
adapter template includes a node resize smoke scenario.

JNR-050 is complete: README/runtime README document resize runtime/adapter boundaries, closeout
evidence is recorded, exact pointer-resize extent parity is split, and this workstream is closed.

The architecture gap is specific: Jellyflow can store and transact node sizes, but runtime adapters
do not yet have a headless resize planner. XyFlow's `XYResizer` already separates much resize math
from React UI, so Jellyflow should capture the renderer-neutral planning part without taking on DOM
handles or renderer smoke.

## Next Task

None. This workstream is closed.

## Decisions Since Opening

- Keep resize planning in `jellyflow-runtime`; adapters own handles, raw input, rendering, and
  pixels.
- Reuse existing `GraphOp::SetNodeSize`; introduce `SetNodePos` and `SetGroupRect` only when later
  slices prove they are needed.
- Public planner/store seams and focused runtime tests are in place before adding conformance schema.
- JNR-020 intentionally did not add resize direction, node origin, extents, parent expansion, or
  conformance fixture vocabulary.
- JNR-030 added direction and origin handling, but split parent/child extent and keep-aspect-ratio
  parity because exact XyFlow behavior depends on pointer start values and clamp distances, not just
  a target-size request.
- JNR-040 uses high-level fixture action vocabulary for resize instead of raw transactions, matching
  the adapter-facing seam adapters should call.
- JNR-050 keeps exact pointer-resize parent/child extent and keep-aspect-ratio parity as a follow-on
  that needs adapter evidence and likely a pointer-session request shape.
- Split exact keep-aspect-ratio, parent/child extent, or NodeResizer UI parity if they broaden the
  first planner task.

## Blockers

- None known.

## Validation To Run

Closeout:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-node-resize-kernel-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-resize-kernel-v1/TASKS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl
git diff --check
```

For lane closeout:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-node-resize-kernel-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-resize-kernel-v1/TASKS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl
git diff --check
```

## Evidence So Far

- 2026-06-02: JNR-010 opened the workstream from XyFlow `XYResizer` source evidence and current
  Jellyflow `SetNodeSize` transaction coverage.
- 2026-06-02: JNR-020 added the minimal pure resize planner and passed `cargo fmt --check`,
  `cargo nextest run -p jellyflow-runtime resize`, and `cargo nextest run -p jellyflow-runtime
  --test public_surface`.
- 2026-06-02: JNR-030 added direction/origin resize planning and passed `cargo fmt --check`,
  `cargo nextest run -p jellyflow-runtime resize`, `cargo nextest run -p jellyflow-runtime
  drag_parent_expansion`, `cargo nextest run -p jellyflow-runtime --test public_surface`, and
  `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`.
- 2026-06-02: JNR-040 added conformance/template resize coverage and passed `cargo fmt --check`,
  `cargo nextest run -p jellyflow-runtime conformance`, `cargo nextest run -p jellyflow-runtime
  adapter_conformance`, `cargo test --manifest-path templates/headless-adapter/Cargo.toml`,
  `cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check`, and
  `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`.
- 2026-06-02: JNR-050 closed the lane and passed `cargo fmt --check`, `cargo nextest run -p
  jellyflow-runtime`, `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`, JSON, and
  diff checks.

## Next Recommended Action

Open a new workstream only if adapter evidence needs exact pointer-resize session parity for
XyFlow parent/child extent clamps or keep-aspect-ratio behavior.
