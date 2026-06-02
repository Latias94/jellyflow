# Jellyflow Node Resize Kernel v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

The workstream is active. It was opened after the closed node drag parent expansion lane.

JNR-010 is complete: the lane scope, non-goals, source coverage, task ledger, campaign record,
milestones, gate set, context manifest, and machine-readable workstream metadata are recorded.

JNR-020 is complete: `runtime::resize` now exposes a renderer-neutral single-node resize request,
constraint, item, plan, pure planner, and `NodeGraphStore` helper. The first slice emits
deterministic `SetNodeSize` transactions, clamps optional min/max bounds, and rejects hidden,
missing, no-op, non-positive, non-finite, invalid-constraint, and contradictory-constraint requests.

The architecture gap is specific: Jellyflow can store and transact node sizes, but runtime adapters
do not yet have a headless resize planner. XyFlow's `XYResizer` already separates much resize math
from React UI, so Jellyflow should capture the renderer-neutral planning part without taking on DOM
handles or renderer smoke.

## Next Task

JNR-030: extend resize planning for left/top position changes, node origin, parent extents, and
child extent restrictions where the contract is clear.

## Decisions Since Opening

- Keep resize planning in `jellyflow-runtime`; adapters own handles, raw input, rendering, and
  pixels.
- Reuse existing `GraphOp::SetNodeSize`; introduce `SetNodePos` and `SetGroupRect` only when later
  slices prove they are needed.
- Public planner/store seams and focused runtime tests are in place before adding conformance schema.
- JNR-020 intentionally did not add resize direction, node origin, extents, parent expansion, or
  conformance fixture vocabulary.
- Split exact keep-aspect-ratio, parent/child extent, or NodeResizer UI parity if they broaden the
  first planner task.

## Blockers

- None known.

## Validation To Run

For JNR-030:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime resize
cargo nextest run -p jellyflow-runtime drag_parent_expansion
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

## Next Recommended Action

Start JNR-030 by deciding the smallest direction/origin slice:

- bottom/right remains size-only and already works;
- left/top should add `SetNodePos` plus `SetNodeSize`;
- node origin and extents should be added only when tests can state the adapter-facing contract
  clearly.
