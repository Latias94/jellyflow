---
type: "Session Handoff"
title: "Node-kit plan and goal setup"
description: "Handoff for the adapter/node-kit boundary plan and the active implementation goal."
tags: ["engineering-memory", "jellyflow", "session", "node-kit", "goal"]
timestamp: 2026-06-19T15:56:24Z
status: "active"
---

# Summary

The adapter/node-kit boundary direction is now fixed as a plan and an active goal. The work should
stay semantic and headless: no shared widget crate, no framework objects in runtime crates, and no
premature ADR unless the boundary later needs to be frozen.

# Verified State

- The reusable plan now exists at [`docs/plans/2026-06-19-003-feat-adapter-node-kit-boundary-plan.md`](../../../plans/2026-06-19-003-feat-adapter-node-kit-boundary-plan.md).
- The plan contains implementation units for runtime schema manifests, first kit families,
  egui cleanup, proof/template coverage, and docs/memory updates.
- A goal is active for the same boundary work, so implementation can start without another planning
  pass.
- Existing guidance still stands: keep `ADR 0008` as the durable boundary, allow fearless
  refactoring, and delete egui-only glue once coverage exists.

# Next Action

Start implementation from the plan units in order, beginning with runtime schema manifest and
recipe descriptors.

# Citations

- [Plan](../../../plans/2026-06-19-003-feat-adapter-node-kit-boundary-plan.md)
- [Current State](../current-state.md)
- [Engineering Log](../log.md)
- [ADR 0008](../../adr/0008-semantic-surface-and-framework-adapter-boundary.md)
