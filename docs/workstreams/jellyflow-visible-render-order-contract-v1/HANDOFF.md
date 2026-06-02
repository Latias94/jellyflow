# Jellyflow Visible Render Order Contract v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

The lane is open and ready for VRO-020.

VRO-010 froze the target state: deepen `runtime::rendering` so adapters can request ordered visible
node ids directly instead of composing `node_render_order()` and `visible_node_ids(viewport_size)`
themselves.

## Next Task

VRO-020:

- add a pure runtime helper for visible node render order;
- add a `NodeGraphStore` helper using current view state and resolved rendering tuning;
- add focused rendering tests and public surface smoke;
- run `cargo fmt --check`, `cargo nextest run -p jellyflow-runtime visible_node_render_order`,
  and `cargo nextest run -p jellyflow-runtime --test public_surface`.

## Guardrails

- Do not add renderer, platform, `wgpu`, egui, or Fret dependencies to runtime/core.
- Do not expand this task into visible edge culling, full scene render plans, or spatial indexing.
- Preserve existing `node_render_order()` and `visible_node_ids(viewport_size)` helpers; this lane
  adds a deeper convenience contract for adapters, not a compatibility-removal migration.
