---
type: "Subagent Finding"
title: "Open GPUI interaction determinism research findings"
tags: ["subagent", "xyflow", "egui-snarl", "open-gpui", "interaction"]
timestamp: 2026-07-05T01:52:32+08:00
status: "accepted"
related_plan: "../../plans/2026-07-05-001-refactor-open-gpui-interaction-determinism-plan.md"
git_branch: "feat/xyflow-product-surface"
---

# Finding

XYFlow and egui-snarl agree on the same product boundary: graph-owned wrappers own selection, node drag, connection/reconnect, pointer priority, and measured internals; user node components own visible internals and explicit handle/control surfaces.

# Evidence

- XYFlow uses `NodeWrapper`, `Handle`, `XYDrag`, and `useUpdateNodeInternals` to separate custom node rendering from graph gesture ownership.
- egui-snarl gives pins, nodes, selection, and dropped-wire menus separate interaction responses. The viewer customizes UI and connection policy without owning global pointer routing.
- Current Jellyflow/Open GPUI failures mapped to Open GPUI canvas and `canvas-jellyflow` host integration, not to `jellyflow-runtime`: Select hit-testing did not prioritize handles, node bodies were default connection endpoints, and fixture switches did not force a next-frame measurement lifecycle.

# Recommendation

- Keep runtime semantic and headless.
- Keep Open GPUI as the only mature adapter target in this phase.
- Put generic pointer priority and endpoint policy in `repo-ref/open-gpui/crates/canvas`.
- Put widget-free evidence in `jellyflow-open-gpui`.
- Put concrete GPUI components, measurement scheduling, and product interaction roles in `canvas-jellyflow`.

# Disposition

Accepted into `docs/plans/2026-07-05-001-refactor-open-gpui-interaction-determinism-plan.md` and partially implemented in the first slice.

# Citations

- [Plan](../../plans/2026-07-05-001-refactor-open-gpui-interaction-determinism-plan.md)
- [Progress note](../progress/2026-07-05-open-gpui-interaction-determinism-progress.md)
