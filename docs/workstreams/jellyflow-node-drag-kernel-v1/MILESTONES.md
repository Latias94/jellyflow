# Jellyflow Node Drag Kernel v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

Exit criteria:

- The workstream documents agree on problem, non-goals, source coverage, and first task.
- ADR 0003 remains the governing renderer-boundary decision.
- ADR 0002 remains the schema-boundary decision.

## M1 - Single Node Drag Transaction

Exit criteria:

- Runtime has a renderer-neutral helper for single-node drag transaction planning.
- Draggable policy, hidden-node behavior, no-op filtering, and graph commit behavior are tested.
- Targeted nextest and `cargo check` pass.

## M2 - Multi-Selection Drag Items

Exit criteria:

- A primary node plus selected nodes can be converted into deterministic drag items.
- Selected-parent filtering mirrors the XyFlow reference behavior.
- Multi-drag updates commit sorted, deterministic `SetNodePos` ops.

## M3 - Snap And Extent Constraints

Exit criteria:

- Snap-to-grid uses a shared offset for multi-drag.
- Global and per-node rect extents clamp movement deterministically.
- Node origin is respected where bounds-based clamping needs it.

## M4 - Gesture Trace And XyFlow Projection

Exit criteria:

- The interaction harness records drag start/update/end events with graph transaction ordering.
- XyFlow-compatible node changes and drag callbacks are covered by adapter conformance fixtures.
- Failure output remains compact enough for agent debugging.

## M5 - Closeout

Exit criteria:

- Runtime package gates pass.
- Workstream evidence is current.
- Follow-ons are split for auto-pan, parent expansion, public fixture format, or renderer smoke
  tests if they remain out of scope.

Status: Complete on 2026-06-01. Closeout evidence is recorded in
`EVIDENCE_AND_GATES.md` and `CLOSEOUT_AUDIT_2026-06-01.md`.
