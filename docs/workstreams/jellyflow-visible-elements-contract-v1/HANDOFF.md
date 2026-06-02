# Jellyflow Visible Elements Contract v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

JVE-010 is complete: the lane is open and source coverage is recorded.

Jellyflow already has most implementation pieces:

- `runtime::utils::get_nodes_inside` performs deterministic linear node queries with sorted output.
- `runtime::viewport::ViewportTransform` converts screen coordinates to canvas coordinates.
- `NodeGraphInteractionState::rendering_interaction` exposes `only_render_visible_elements` and
  spatial tuning.

The missing seam is an adapter-facing runtime/store contract that combines those pieces like
XyFlow's `useVisibleNodeIds`.

## Next Task

JVE-020: add a renderer-neutral visible node id planner and store helper using viewport transform,
logical viewport size, node-origin policy, and `only_render_visible_elements`.

## Decisions Since Opening

- Scope v1 to visible node ids only.
- Keep visible edge ids as a follow-on because edge visibility needs endpoint/path/AABB policy.
- Keep real spatial indexing as a follow-on until adapter workload evidence proves the linear
  fallback is too slow.
- Use the current runtime linear scan as the behavior baseline so a future index can swap behind the
  same contract.

## Validation To Run

For JVE-020:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime visible_node
cargo nextest run -p jellyflow-runtime --test public_surface
```

For closeout:

```bash
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-visible-elements-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-visible-elements-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-visible-elements-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-visible-elements-contract-v1/CONTEXT.jsonl
git diff --check
```

## Next Recommended Action

Start JVE-020 by adding a small runtime visible module. The first tests should prove:

- culling disabled returns all non-hidden node ids;
- culling enabled returns partially visible node ids for the current viewport;
- invalid viewport size or transform returns no visible nodes instead of panicking;
- store helper reads `only_render_visible_elements` from resolved runtime tuning.
