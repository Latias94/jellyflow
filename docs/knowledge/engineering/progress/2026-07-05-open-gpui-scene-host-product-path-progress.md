---
type: "Work Progress"
title: "Open GPUI scene-host product path cleanup"
description: "Second atomic node scene slice: canvas-jellyflow host consumes prepared scene records before bootstrap fallback."
timestamp: 2026-07-05T12:37:26Z
tags: ["open-gpui", "canvas-jellyflow", "scene-host", "ce-work"]
status: "active"
related_plan: "docs/plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md"
git_branch: "feat/xyflow-product-surface"
---

# Summary

The second atomic node scene slice cleaned up the concrete `canvas-jellyflow` host path.
The product renderer no longer exposes the old `render_node_surfaces` / `surface_render_nodes` vocabulary.
It now renders scene-owned node widgets from `NodeSceneHostRecords`, where `PreparedSceneFrame` is the preferred source.

# Details

- `JellyflowCanvasView` now caches the latest `CanvasSceneFrame` produced by the canvas prepaint frame callback.
- The host renders node widgets from prepared scene record groups first, recomputes from the last canvas bounds only as a secondary scene source, and uses document z-order only as `InitialDocumentBootstrap`.
- Editor events, shortcut mutations, tool switches, fixture switches, viewport changes, and store refreshes invalidate the cached scene so drag/connection paths cannot use stale positions.
- The example test surface now names the bootstrap fallback explicitly, which prevents future code from treating document-only sorting as the product correctness path.

# Next Action

- Continue U8 product renderer polish: inspect Dify/shader/ERD/mind-map text clipping and remove any remaining layout estimates that are not backed by measured regions.
- Run native manual review after the next renderer cleanup: overlap ordering, first paint without pointer movement, port drag, node drag, and control shielding.

# Citations

- [Open GPUI Atomic Node Scene Plan](../../plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md)
- [canvas-jellyflow main](../../../repo-ref/open-gpui/examples/canvas-jellyflow/src/main.rs)
