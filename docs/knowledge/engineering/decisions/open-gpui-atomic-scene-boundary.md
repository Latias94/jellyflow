---
type: "Decision"
title: "Open GPUI atomic scene boundary"
description: "Canvas scene layers and widget-free adapter plans are the new boundary for rich Open GPUI node UI."
timestamp: 2026-07-05T12:10:58Z
tags: ["open-gpui", "canvas", "jellyflow", "architecture", "node-ui"]
status: "active"
related_plan: "docs/plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md"
git_branch: "feat/xyflow-product-surface"
---

# Decision

Open GPUI rich node UI should be integrated through an atomic scene/display-list boundary.
The long-term shape is not "one canvas child plus one later widget overlay."
Each graph record needs one ordered group that can contain body/backplate, concrete GPUI internals, node-local chrome, ports, and selected/hover affordances.

Ownership stays split:

- `jellyflow-runtime` remains headless and owns semantic node descriptors, anchors, slots, repeatables, actions, and measurement concepts.
- `jellyflow-open-gpui` remains widget-free and owns surface planning, measured region roles, port-handle plans, and product evidence contracts.
- `repo-ref/open-gpui/crates/canvas` owns generic scene phases, record ordering, canvas painting phases, geometry facts, and pointer-owner routing.
- `repo-ref/open-gpui/examples/canvas-jellyflow` owns concrete GPUI elements, layout, focus/popups, host lifecycle, and product renderer polish.

# Context

The observed bug was not only visual polish.
When nodes overlap, a lower node's GPUI internals can remain visible above an upper node's canvas body because every custom node surface is appended after the entire canvas.
Selection outlines, ports, and reconnect affordances also fight that same global-layer split.

The fix follows the mature-library lesson from XYFlow and egui-snarl:
the graph owns wrapper/measurement/handles/drag semantics, user or host code owns internals, and layout-produced geometry should become the same source for visuals, hit targets, and endpoints.

# Alternatives

- Keep the global GPUI overlay and add larger opaque wrappers.
  This reduces some leak-through but cannot interleave one node's body above another node's internals by record z order.
- Move widget ownership into `jellyflow-runtime`.
  This would violate the headless contract and make egui/Dioxus/Open GPUI share the wrong abstraction.
- Create a cross-framework widget crate now.
  This is premature; the shared contract should be semantic descriptors and measured roles, while concrete widgets remain adapter-local.
- Use padding, text-fit estimates, or hidden handle hacks as proof.
  These are not library contracts and should be removed when scene/measurement facts exist.

# Consequences

- Future Open GPUI work should prefer typed scene phases and measured roles over z-index constants or wrapper heuristics.
- Product readiness should require fresh measured internals where the product gate needs layout-pass facts; projection fallback remains degraded evidence.
- Concrete product UI polish belongs in `canvas-jellyflow` or a future Open GPUI host crate, not in `jellyflow-runtime`.
- egui and Dioxus can later map the same semantic contracts locally, but they should not be treated as mature adapter targets in this slice.

# Citations

- [Open GPUI Atomic Node Scene Plan](../../plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md)
- [Node UI Kit Component Contract](node-ui-kit-component-contract.md)
- [Open GPUI Node Component Kit](open-gpui-node-component-kit.md)
