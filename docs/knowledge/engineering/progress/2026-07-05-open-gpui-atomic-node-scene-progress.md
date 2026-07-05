---
type: "Work Progress"
title: "Open GPUI atomic node scene first slice"
description: "First implementation slice for atomic node scene layering and Open GPUI adapter planning."
timestamp: 2026-07-05T12:10:58Z
tags: ["open-gpui", "canvas", "jellyflow", "atomic-scene", "ce-work"]
status: "active"
related_plan: "docs/plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md"
git_branch: "feat/xyflow-product-surface"
---

# Summary

The first implementation slice of the Open GPUI atomic node scene plan is in place.
The core direction is now executable in code: rich Jellyflow node UI should no longer be modeled as one global canvas plus one global GPUI overlay.
Instead, Open GPUI canvas exposes typed scene phases and record groups, while `canvas-jellyflow` places concrete GPUI node internals into those record groups.

# Details

- `repo-ref/open-gpui/crates/canvas` now has a `CanvasSceneFrame` with typed phases for document underlay, behind-node edges, record body, record widget, record chrome, above-node edges, tool chrome, and host portals.
- The Open GPUI painter/view gained phase-aware entry points so a host can compose canvas bodies, widget internals, and tool chrome without forcing all widgets above all records.
- Generic select-tool pointer routing now uses `CanvasPointerOwner`, which makes source handles, reconnect handles, transforms, node drag, record hits, and pane selection explicit.
- `jellyflow-open-gpui` now owns widget-free adapter facts for `OpenGpuiNodeSurfacePlan`, `OpenGpuiInteractionRegionRole`, and `OpenGpuiPortHandlePlan`.
- `canvas-jellyflow` now builds product node surfaces from the scene record order when canvas bounds are available, gives every node surface an opaque full-node backplate, and derives port-handle product evidence from the adapter port plan.

This is a meaningful architecture slice but not the full plan closeout.
The remaining productization work is to remove more compatibility overlay naming/shape, polish renderer layouts, and run native manual overlap/first-paint review after the next cleanup slice.

# Next Action

- Commit this slice separately in Jellyflow root and local `repo-ref/open-gpui`.
- Continue U4/U8 cleanup: make the example host surface path visibly scene-owned, reduce compatibility fallback usage, and delete any remaining wrapper/fit/projection hacks that are no longer needed.
- Launch `canvas-jellyflow` manually and re-check overlapping Dify/shader/ERD/mind-map nodes for no leak-through, no delayed internals, reliable port hits, and predictable body drag.

# Citations

- [Open GPUI Atomic Node Scene Plan](../../plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md)
- [Canvas scene frame](../../../repo-ref/open-gpui/crates/canvas/src/gpui/frame.rs)
- [Canvas phase painter](../../../repo-ref/open-gpui/crates/canvas/src/gpui/painter.rs)
- [Open GPUI adapter measurement contract](../../../crates/jellyflow-open-gpui/src/measurement.rs)
- [Open GPUI adapter renderer plan](../../../crates/jellyflow-open-gpui/src/renderer.rs)
