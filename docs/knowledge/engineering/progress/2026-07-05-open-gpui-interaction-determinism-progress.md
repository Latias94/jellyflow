---
type: "Work Progress"
title: "Open GPUI interaction determinism refactor progress"
tags: ["open-gpui", "canvas", "interaction", "ce-work"]
timestamp: 2026-07-05T01:52:32+08:00
status: "active"
related_plan: "../../plans/2026-07-05-001-refactor-open-gpui-interaction-determinism-plan.md"
git_branch: "feat/xyflow-product-surface"
---

# Summary

The Open GPUI interaction determinism plan has started implementation across the root Jellyflow repo and the local `repo-ref/open-gpui` main fork.

# Completed In This Slice

- Created the implementation-ready plan at `docs/plans/2026-07-05-001-refactor-open-gpui-interaction-determinism-plan.md`.
- Kept `jellyflow-runtime` headless. No GPUI pointer routing or widget state was moved into runtime.
- In `repo-ref/open-gpui/crates/canvas`, made whole-node connection endpoints explicit opt-in through `CanvasNodeInteractionPolicy::node_accepts_connection_endpoint`.
- Updated connection hit-testing and geometry facts so node bodies are not valid source/target endpoints by default.
- Shared Connect-tool connection lifecycle helpers and let Select mode start a connection from an explicit source handle on first pointer down.
- Added reducer coverage for handle-first Select behavior, default node-body endpoint rejection, and policy-enabled whole-node endpoints.
- In `repo-ref/open-gpui/examples/canvas-jellyflow`, split `measurement_refresh_requested` from `measurement_frame_pending`, added a single next-frame scheduling helper, and made product gallery fixture switches request deterministic render/measurement without pointer movement.
- Started the host-local interaction role cleanup in `node_component_kit`: product surface pointer forwarding, control shield, port handle, readable content, and overflow action are now explicit roles; old handle/control helper aliases were removed.
- Removed the unused `CanvasPoint` import from `crates/jellyflow-open-gpui/src/testing.rs`.

# Still Open

- U5-U7 are not complete: visible port marker vs hit-region evidence, richer invalid-hover/dropped-wire evidence, and full structured first-pointer reports still need another implementation slice.
- `canvas-jellyflow` still carries intentionally public component-context methods that are unused by the example binary but remain useful as the host component seam; do not delete them without deciding to shrink that seam.
- The latest diff-review subagent findings were folded into this slice before verification; the stale agent handle was interrupted after its conclusions had already been captured.

# Citations

- [Plan](../../plans/2026-07-05-001-refactor-open-gpui-interaction-determinism-plan.md)
- [Current state rollup](../current-state.md)
