---
type: "Work Progress"
title: "Open GPUI U5-U7 interaction evidence completed"
description: "Work Progress for Open GPUI U5-U7 interaction evidence completed."
timestamp: 2026-07-04T19:02:03Z
tags: ["open-gpui", "jellyflow", "ce-work", "interaction"]
related_plan: "docs/plans/2026-07-05-001-refactor-open-gpui-interaction-determinism-plan.md"
git_branch: "feat/xyflow-product-surface"
---

# Summary

U5-U7 interaction determinism evidence is implemented for the Open GPUI product
gallery path. The adapter report now carries explicit port-handle, first-pointer,
and connection-release evidence instead of relying on broad booleans or visual
inspection.

# Details

- `crates/jellyflow-open-gpui/src/testing.rs` adds hard gates for visible port
  markers, measured anchors, canvas handles, endpoint hit-tests, disabled/missing
  ports, first pointer ownership, no-pointer readiness, dynamic handle freshness,
  and dropped-wire release ownership.
- `repo-ref/open-gpui/examples/canvas-jellyflow/src/main.rs` feeds those gates
  from widget-free probes over product fixtures, measured transform fixtures,
  connection/reconnect sequences, and dropped-wire actions.
- The disabled-port probe now adds a unique unconnected disabled product port,
  so the test verifies non-connectability through the real graph/project/endpoint
  path without corrupting existing edge endpoints or reusing a conflicting demo
  id.
- `visual_regression::repeatable_edits_update_anchor_identity` is exposed within
  the example crate so first-pointer evidence can prove dynamic handle freshness.

# Next Action

Commit root Jellyflow changes and `repo-ref/open-gpui` example changes
separately, then continue with any remaining manual native smoke review.

# Citations

- [Plan](../../plans/2026-07-05-001-refactor-open-gpui-interaction-determinism-plan.md)
- `crates/jellyflow-open-gpui/src/testing.rs`
- `repo-ref/open-gpui/examples/canvas-jellyflow/src/main.rs`
- `repo-ref/open-gpui/examples/canvas-jellyflow/src/visual_regression.rs`
