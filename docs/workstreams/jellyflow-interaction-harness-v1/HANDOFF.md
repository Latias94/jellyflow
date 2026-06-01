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

## Next Task

JIH-030: add the first renderer-neutral selection-box fixture and, if needed, a minimal headless
selection helper that turns a canvas box into ordered selection state.

## Decisions Since Last Update

- Keep the first harness private to `jellyflow-runtime` tests.
- Use explicit normalized traces instead of snapshot-test dependencies.
- Prove the harness through existing adapter conformance before adding new interaction kernels.
- Keep renderer-specific smoke tests out of this lane.
- The first harness trace covers graph commits, view changes, and connect gesture lifecycle events.
- The next task should test selection-box behavior through the harness before extracting public
  fixture APIs.

## Blockers

- None known.

## Follow-On Candidates

- Public fixture format after several private harness scenarios settle.
- Selection-box kernel fixtures.
- Drag, connect/reconnect, resize, and pan/zoom gesture kernels.
- Renderer adapter smoke tests in future adapter crates.
