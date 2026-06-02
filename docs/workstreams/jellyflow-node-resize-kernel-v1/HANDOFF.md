# Jellyflow Node Resize Kernel v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

The workstream is active. It was opened after the closed node drag parent expansion lane.

JNR-010 is complete: the lane scope, non-goals, source coverage, task ledger, campaign record,
milestones, gate set, context manifest, and machine-readable workstream metadata are recorded.

The architecture gap is specific: Jellyflow can store and transact node sizes, but runtime adapters
do not yet have a headless resize planner. XyFlow's `XYResizer` already separates much resize math
from React UI, so Jellyflow should capture the renderer-neutral planning part without taking on DOM
handles or renderer smoke.

## Next Task

JNR-020: add the first renderer-neutral node resize request/plan that emits deterministic
`SetNodeSize` transactions for a single node with min/max bounds.

## Decisions Since Opening

- Keep resize planning in `jellyflow-runtime`; adapters own handles, raw input, rendering, and
  pixels.
- Reuse existing `GraphOp::SetNodeSize`, with `SetNodePos` and `SetGroupRect` only when later
  slices prove they are needed.
- Start with public planner/store seams and focused runtime tests before adding conformance schema.
- Split exact keep-aspect-ratio, parent/child extent, or NodeResizer UI parity if they broaden the
  first planner task.

## Blockers

- None known.

## Validation To Run

For JNR-020:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime resize
cargo nextest run -p jellyflow-runtime --test public_surface
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

## Next Recommended Action

Start JNR-020 with red tests for:

- accepted bottom/right single-node resize producing `SetNodeSize`;
- no-op or invalid size requests producing no plan;
- min/max bounds clamping or rejection through the public resize planner interface.
