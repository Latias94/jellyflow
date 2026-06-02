# Resize lifecycle callbacks

## Goal

Add the first headless resize lifecycle callback contract for `jellyflow-runtime`, closing the
callback portion of the XyFlow resize parity gap while preserving the existing pointer resize
planner and store commit API.

This task should make resize start/update/end observable through transient gesture events,
XyFlow-shaped callback projection, and reusable conformance/template traces. It should not turn the
core graph or store into a stateful DOM resize-session engine.

## Confirmed Facts

- The pointer resize planner already exists under `runtime::resize` and commits through
  `NodeGraphStore::apply_node_pointer_resize`.
- `runtime::events::NodeGraphGestureEvent` already models transient UI gesture lifecycle for drag,
  connect, and viewport movement.
- `runtime::xyflow::callbacks::NodeGraphGestureCallbacks` dispatches gesture events into
  XyFlow-shaped callback hooks.
- The conformance runner already supports `ConformanceAction::EmitGesture`, so resize lifecycle
  tests can reuse the existing explicit gesture emission path.
- ADR 0004 allows resize start/update/end gesture events and callback projection, while keeping
  stateful lifecycle policy out of persisted graph state.
- XyFlow `XYResizer` calls `onResizeStart` at drag start, may gate update through `shouldResize`,
  and only calls `onResizeEnd` after an actual resize update was accepted.

## Requirements

- Add renderer-neutral resize lifecycle event payloads for start, update, and end.
- Add resize lifecycle variants to `NodeGraphGestureEvent`.
- Add no-op-by-default resize lifecycle methods to `NodeGraphGestureCallbacks`.
- Dispatch resize gesture events through `runtime::xyflow::callbacks`.
- Add resize callback trace variants to conformance callback recording.
- Add at least one conformance scenario proving resize start, committed pointer resize update, and
  resize end callback order.
- Update the headless adapter template smoke suite so adapter authors can see the lifecycle
  contract around pointer resize.
- Add public-surface coverage for the new event/callback types.
- Preserve existing pointer resize planner behavior, target-size resize behavior, and current
  conformance trace behavior for non-resize gestures.

## Acceptance Criteria

- [x] Resize lifecycle events serialize through `NodeGraphGestureEvent` using the existing
  serde-tagged event shape.
- [x] `NodeGraphGestureCallbacks` exposes `on_node_resize_start`, `on_node_resize`, and
  `on_node_resize_end` with default no-op implementations.
- [x] `install_callbacks` dispatches resize start/update/end gesture events into those hooks.
- [x] Conformance callback traces include `NodeResizeStart`, `NodeResize`, and `NodeResizeEnd`.
- [x] A runner test verifies expected ordering:
  gesture start -> resize start callback -> graph commit -> commit callbacks -> gesture update ->
  resize callback -> gesture end -> resize end callback.
- [x] Template smoke coverage includes the same lifecycle boundary around pointer resize.
- [x] Public API coverage references the new event payloads and gesture enum variants.
- [x] `cargo fmt --check` passes.
- [x] Focused `cargo nextest` checks for `jellyflow-runtime` conformance/public-surface resize
  coverage pass.
- [x] Trellis task validation passes.

## Out of Scope

- Adding a stateful resize-session API to `NodeGraphStore`.
- Implementing XyFlow `shouldResize` gating.
- Adding DOM/d3 pointer integration, React `NodeResizer` props, or visual resize handles.
- Changing pointer resize math, constraints, child correction, or parent expansion behavior.
- Persisting lifecycle state in graph/editor files.

## Notes

- Recommended first slice: lifecycle event/callback vocabulary plus conformance/template coverage
  around explicit `emit_gesture` and the existing pointer resize commit action. `shouldResize` and a
  full session orchestrator should be a follow-up only if adapter needs justify the extra state.
