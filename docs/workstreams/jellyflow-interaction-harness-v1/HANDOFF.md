# Jellyflow Interaction Harness v1 - Handoff

Status: Active
Last updated: 2026-06-01

## Current State

The workstream is open as the follow-on to geometry/spatial and ADR 0003 adapter testing.

JIH-010 is complete: the lane scope, non-goals, source coverage, task ledger, milestones, gate set,
context manifest, and machine-readable workstream metadata are recorded.

JIH-020 is complete: `crates/jellyflow-runtime/src/runtime/tests/harness.rs` now provides a private
runtime test harness around a real `NodeGraphStore`. It records normalized graph commit, view, and
gesture traces and adapter conformance scenarios use scenario-aware trace assertions.

JIH-030 is complete: `runtime::selection` now provides renderer-neutral `compute_selection_box` and
`NodeGraphStore::apply_selection_box`. The harness-backed fixtures cover replacement selection,
additive union, hidden-node exclusion, selectable policy filtering, connected edge selection,
deterministic sorting, and emitted selection events.

## Next Task

JIH-040: extend the harness to cover at least one drag or connect/reconnect gesture fixture with
expected transactions and callbacks.

## Decisions Since Last Update

- Keep the first harness private to `jellyflow-runtime` tests.
- Use explicit normalized traces instead of snapshot-test dependencies.
- Prove the harness through existing adapter conformance before adding new interaction kernels.
- Keep renderer-specific smoke tests out of this lane.
- The first harness trace covers graph commits, view changes, and connect gesture lifecycle events.
- The next task should test selection-box behavior through the harness before extracting public
  fixture APIs.
- Selection-box behavior now uses existing lookups, policy resolution, and `get_nodes_inside`
  rather than introducing a renderer-specific path.
- `NodeGraphStore::apply_selection_box` updates view state through `set_selection`, so subscribers
  observe the same `ViewChange::Selection` path as adapters.

## Blockers

- None known.

## Follow-On Candidates

- Public fixture format after several private harness scenarios settle.
- Drag, connect/reconnect, resize, and pan/zoom gesture kernels.
- Renderer adapter smoke tests in future adapter crates.
