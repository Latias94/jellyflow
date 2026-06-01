# Jellyflow Node Drag Kernel v1 - TODO

Status: Active
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JND-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-node-drag-kernel-v1]
  Goal: Open the node drag kernel lane and freeze the first execution contract.
  Validation: DESIGN.md, TODO.md, MILESTONES.md, EVIDENCE_AND_GATES.md, CONTEXT.jsonl, WORKSTREAM.json, and HANDOFF.md exist and agree.
  Evidence: `docs/workstreams/jellyflow-node-drag-kernel-v1/DESIGN.md`
  Context: `docs/workstreams/jellyflow-node-drag-kernel-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Opened from the interaction harness closeout follow-on and ADR 0003's renderer-free testing decision.

## M1 - Single Node Drag Transaction

- [x] JND-020 [owner=codex] [deps=JND-010] [scope=crates/jellyflow-runtime/src/runtime/drag.rs,crates/jellyflow-runtime/src/runtime/tests/drag.rs,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Add the first renderer-neutral drag helper that plans and applies a single-node `SetNodePos` transaction from canvas-space intent.
  Validation: `cargo nextest run -p jellyflow-runtime drag`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: Fixture proves draggable policy, hidden-node exclusion, no-op filtering, deterministic transaction label/op, and committed graph state.
  Context: `docs/workstreams/jellyflow-node-drag-kernel-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Added public `runtime::drag` single-node planning/apply helpers, exposed them through the public surface check, and added harness-backed fixtures for successful commit trace plus missing/hidden/non-draggable/no-op/non-finite/global-disabled no-commit cases.

## M2 - Multi-Selection Drag Items

- [x] JND-030 [owner=codex] [deps=JND-020] [scope=crates/jellyflow-runtime/src/runtime/drag.rs,crates/jellyflow-runtime/src/runtime/tests/drag.rs,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Extend drag planning to build deterministic drag items from the primary node plus selected nodes.
  Validation: `cargo nextest run -p jellyflow-runtime drag`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: Fixture covers selected-node co-dragging, primary node inclusion, sorted output, non-draggable nodes, and selected-parent child filtering.
  Context: `docs/workstreams/jellyflow-node-drag-kernel-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Extended `NodeGraphStore::plan_node_drag` to co-drag current selected nodes with the primary node, added ordered `NodeDragItem` output, deterministic sorted `SetNodePos` ops, selected-group child filtering for Jellyflow parent semantics, and public surface coverage for `NodeDragItem`.

## M3 - Snap And Extent Constraints

- [x] JND-040 [owner=codex] [deps=JND-030] [scope=crates/jellyflow-runtime/src/runtime/drag.rs,crates/jellyflow-runtime/src/runtime/tests/drag.rs]
  Goal: Add snap-to-grid and movement extent handling without renderer dependencies.
  Validation: `cargo nextest run -p jellyflow-runtime drag`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: Fixture covers shared snap offset for multi-drag, global node extent, per-node rect extent, node origin, and deterministic clamping.
  Context: `docs/workstreams/jellyflow-node-drag-kernel-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Added shared snap-offset planning, global extent group clamping for multi-drag, per-node rect extent clamping, node-origin-aware bounds calculation, and basic parent-group extent resolution without renderer dependencies.

## M4 - Gesture Trace And XyFlow Projection

- [x] JND-050 [owner=codex] [deps=JND-040] [scope=crates/jellyflow-runtime/src/runtime/events,crates/jellyflow-runtime/src/runtime/tests/harness.rs,crates/jellyflow-runtime/src/runtime/tests/adapter_conformance.rs,crates/jellyflow-runtime/src/runtime/xyflow,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Record drag start/update/end behavior through the interaction harness and XyFlow compatibility callbacks.
  Validation: `cargo nextest run -p jellyflow-runtime adapter_conformance`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: Fixture proves pointer intent, graph transaction, gesture ordering, node-change projection, and drag callback payloads.
  Context: `docs/workstreams/jellyflow-node-drag-kernel-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Added renderer-neutral node drag gesture events with pointer payloads, wired XyFlow-compatible drag start/update/end callbacks, extended the interaction harness trace, and added adapter conformance plus public surface coverage.

## M5 - Closeout

- [ ] JND-060 [owner=codex] [deps=JND-050] [scope=docs/workstreams/jellyflow-node-drag-kernel-v1,README.md,crates/jellyflow-runtime/README.md]
  Goal: Document the drag kernel strategy, record fresh evidence, and close the lane or split follow-ons.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`; `jq empty docs/workstreams/jellyflow-node-drag-kernel-v1/WORKSTREAM.json`; `git diff --check`.
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: EVIDENCE_AND_GATES.md and HANDOFF.md.
  Context: `docs/workstreams/jellyflow-node-drag-kernel-v1/CONTEXT.jsonl`
