# Jellyflow Visible Render Order Contract v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

VRO-030 is complete. Ordered visible node ids are now available through runtime/store APIs and
through `ConformanceAction::assert_visible_node_render_order`.

The headless adapter template suite now has 9 scenarios, including:

- `template visible node ids`;
- `template visible node render order`.

## Next Task

VRO-040:

- document the ordered visible-node render helper in root/runtime docs;
- update `CONTEXT.md` with the closed lane and follow-ons;
- run final package, clippy, metadata, and diff gates;
- close `WORKSTREAM.json`, `TODO.md`, `TASKS.jsonl`, `EVIDENCE_AND_GATES.md`, and this handoff.

## Guardrails

- Do not add renderer, platform, `wgpu`, egui, or Fret dependencies to runtime/core.
- Do not expand this task into visible edge culling, full scene render plans, or spatial indexing.
- Preserve existing `node_render_order()` and `visible_node_ids(viewport_size)` helpers; this lane
  adds a deeper convenience contract for adapters, not a compatibility-removal migration.
