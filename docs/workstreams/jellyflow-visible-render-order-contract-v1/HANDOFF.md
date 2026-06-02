# Jellyflow Visible Render Order Contract v1 - Handoff

Status: Closed
Last updated: 2026-06-02

## Current State

The workstream is closed.

Delivered contract:

- `runtime::rendering::resolve_visible_node_render_order`;
- `NodeGraphStore::visible_node_render_order(viewport_size)`;
- `ConformanceAction::assert_visible_node_render_order`;
- headless adapter template scenario `template visible node render order`.

The helper composes existing visible-node culling and node render-order semantics inside
`runtime::rendering`, so adapters do not have to duplicate hidden-node filtering, draw order,
selected-node elevation, or `only_render_visible_elements` behavior.

## Follow-Ons

- Visible edge culling only after adapter evidence settles endpoint/path/AABB semantics.
- Full scene render plans or render batches only after adapter evidence proves ordered visible node
  ids plus existing group/edge order helpers are insufficient.
- Renderer smoke harnesses stay in future adapter crates such as `jellyflow-wgpu`,
  `jellyflow-egui`, or `jellyflow-fret`.
- Real spatial indexing stays behind `NodeGraphSpatialIndexTuning` until visible-node workloads
  show linear scans are too slow.

## Guardrails

- Do not add renderer, platform, `wgpu`, egui, or Fret dependencies to runtime/core.
- Do not expand this task into visible edge culling, full scene render plans, or spatial indexing.
- Preserve existing `node_render_order()` and `visible_node_ids(viewport_size)` helpers; this lane
  adds a deeper convenience contract for adapters, not a compatibility-removal migration.
