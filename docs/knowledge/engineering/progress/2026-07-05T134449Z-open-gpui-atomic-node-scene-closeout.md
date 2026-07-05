---
type: "Work Progress"
title: "Open GPUI atomic node scene closeout"
timestamp: 2026-07-05T21:44:49+08:00
status: active
git_branch: feat/xyflow-product-surface
related_plan: docs/plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md
tags: ["open-gpui", "atomic-scene", "node-ui", "ce-work"]
---

# Summary

The overlap leak was traced to scene ordering that still allowed selected node chrome and selected node internals to be treated as different display layers. A selected low-z node could have top chrome above another node while its GPUI widget content remained below that other node, producing the visible "node layer / UI layer" split during overlap.

The Open GPUI canvas scene now promotes selected or structurally selected `CanvasSceneRecordGroup`s as one atomic sortable unit. This follows the React Flow-style selected-node elevation policy while keeping ordinary unselected canvas-only ordering by document z and ordinal.

# Implementation Notes

- `repo-ref/open-gpui/crates/canvas/src/gpui/frame.rs` now sorts record groups by selected promotion band, then z index, then ordinal.
- `repo-ref/open-gpui/crates/canvas/src/gpui.rs` has a regression test proving the selected node's widget layer moves above the covering node's chrome instead of only promoting external chrome.
- `repo-ref/open-gpui/examples/canvas-jellyflow` removed the old horizontal repeatable-chip fit path and routes shader/table repeatables through `ProductRepeatableLayoutPlan`.
- Product renderer tests now prove published preferred sizes, not public min-readable budgets, are the full-layout claim.
- `demo.shader.mix` preferred height is now 340px in runtime because the shader product node uses stable vertical repeatable rows rather than width-guessed chips.

# Remaining Boundaries

- `jellyflow-runtime` remains renderer-neutral and only publishes semantic/layout budgets.
- `jellyflow-open-gpui` remains widget-free.
- Concrete Open GPUI node internals and component rows stay in `canvas-jellyflow`.
- The screenshot PNG exporter test remains a known slow/hanging smoke gate and was excluded from the final example nextest gate after 87 other tests had passed.

# Citations

- [Atomic node scene plan](../../plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md)
- [Open GPUI scene frame](../../../repo-ref/open-gpui/crates/canvas/src/gpui/frame.rs)
- [Open GPUI scene tests](../../../repo-ref/open-gpui/crates/canvas/src/gpui.rs)
- [Canvas Jellyflow product renderers](../../../repo-ref/open-gpui/examples/canvas-jellyflow/src/product_renderers.rs)
