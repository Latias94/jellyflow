# Resize Lifecycle Callback Research

## Repository Evidence

- `crates/jellyflow-runtime/src/runtime/events/gesture.rs` currently has gesture variants for
  connect, node drag, and viewport move. Resize lifecycle has no event payloads yet.
- `crates/jellyflow-runtime/src/runtime/events/node_drag.rs` provides the closest local shape:
  start/update/end payloads plus an end-outcome enum.
- `crates/jellyflow-runtime/src/runtime/xyflow/callbacks/traits.rs` has
  `NodeGraphGestureCallbacks` for transient UI lifecycle hooks. It is the right boundary for
  resize lifecycle projection.
- `crates/jellyflow-runtime/src/runtime/xyflow/callbacks/dispatch/gesture.rs` dispatches
  `NodeGraphGestureEvent` variants to callback methods. Resize support can extend this match
  without changing store subscription semantics.
- `crates/jellyflow-runtime/src/runtime/conformance/scenario/action.rs` already has
  `ConformanceAction::EmitGesture`, so no resize-specific lifecycle action is required for the
  first slice.
- `crates/jellyflow-runtime/src/runtime/conformance/scenario/callback_recorder.rs` records
  gesture callbacks into `ConformanceCallbackEvent`; adding resize variants there will make
  conformance traces reusable by adapters.
- `templates/headless-adapter/src/lib.rs` already contains node drag and resize smoke scenarios.
  The resize scenario can be extended to demonstrate lifecycle ordering around pointer resize.
- `crates/jellyflow-runtime/tests/public_surface.rs` already checks public exposure for runtime
  events, resize planner APIs, and XyFlow callback types.

## XyFlow Evidence

- `repo-ref/xyflow/packages/system/src/xyresizer/XYResizer.ts` calls `onResizeStart` at drag start
  after collecting current node values.
- During drag update, XyFlow computes next geometry, calls `shouldResize` with the next values, and
  skips `onResize` plus graph changes when `shouldResize` returns false.
- XyFlow calls `onResizeEnd` and the internal `onEnd` hook only if at least one resize update was
  accepted.

## ADR/Task Evidence

- `docs/adr/0004-resize-containment-and-lifecycle-boundary.md` explicitly allows headless resize
  gesture events and `runtime::xyflow` callback projection, while keeping lifecycle state out of
  the persisted graph.
- `.trellis/tasks/archive/2026-06/06-02-pointer-resize-parity/` documents the previous slice:
  pointer resize math, store action, conformance, and template coverage are already in place.

## Implications

- First-slice lifecycle support should be additive and event-driven.
- The update event should carry the node id, resize direction, pointer, resulting position, and
  resulting size, so callback consumers can observe the same geometry that was committed without
  depending on private planner internals.
- End events should include an outcome enum analogous to `NodeDragEndOutcome`, but the conformance
  happy path can use `Committed`.
- `shouldResize` should be deferred because it changes planning/commit orchestration rather than
  merely exposing lifecycle observation.
