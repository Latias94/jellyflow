# Jellyflow Visible Render Order Contract v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

VRO-020 is complete. The runtime now exposes ordered visible node ids through
`runtime::rendering::resolve_visible_node_render_order` and
`NodeGraphStore::visible_node_render_order(viewport_size)`.

The helper composes existing visible-node culling and node render-order semantics inside
`runtime::rendering`, so adapters do not have to duplicate hidden-node filtering, draw order, or
selected-node elevation logic.

## Next Task

VRO-030:

- add a conformance action for asserting ordered visible node ids;
- add runner checks and focused conformance tests;
- add adapter-conformance/template coverage;
- run `cargo fmt --check`, `cargo nextest run -p jellyflow-runtime conformance`, `cargo nextest
  run -p jellyflow-runtime adapter_conformance`, `cargo test --manifest-path
  templates/headless-adapter/Cargo.toml`, and `cargo run --manifest-path
  templates/headless-adapter/Cargo.toml -- check`.

## Guardrails

- Do not add renderer, platform, `wgpu`, egui, or Fret dependencies to runtime/core.
- Do not expand this task into visible edge culling, full scene render plans, or spatial indexing.
- Preserve existing `node_render_order()` and `visible_node_ids(viewport_size)` helpers; this lane
  adds a deeper convenience contract for adapters, not a compatibility-removal migration.
