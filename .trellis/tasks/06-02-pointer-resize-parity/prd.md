# Pointer resize session parity

## Goal

Add the first headless pointer-driven node resize contract for `jellyflow-runtime`, closing the
largest runtime resize gap found in the XyFlow parity review while preserving the existing
target-size resize API.

This task should make resize behavior derivable from canvas-space pointer movement, not from a
renderer or DOM event stream. Adapters remain responsible for pointer capture, coordinate
conversion, zoom transforms, and visual handles.

## Requirements

- Preserve the existing `NodeResizeRequest` target-size API and current resize tests.
- Add a renderer-neutral pointer resize planning API under `runtime::resize` and, where store state
  is required, a `NodeGraphStore` entry point.
- Accept canvas-space start/current pointer positions plus a resize control direction.
- Support all eight control directions already represented by `NodeResizeDirection`.
- Support min/max size constraints, node origin, finite-value validation, no-op handling, and
  hidden-node rejection consistently with existing resize behavior.
- Add keep-aspect-ratio behavior compatible with XyFlow's `getDimensionsAfterResize` semantics for
  compatible Jellyflow inputs.
- Support `NodeExtent::Rect` and `NodeExtent::Parent` clamping where Jellyflow can resolve a parent
  `Group` rect. `NodeExtent::Parent` must continue to respect `expand_parent`: if parent expansion
  is enabled, the pointer resize planner should not clamp to that parent in this slice.
- Keep renderer, browser, Fret, `wgpu`, `winit`, and UI dependencies out of `jellyflow-runtime`.
- Add focused runtime tests and adapter conformance coverage for the new adapter-facing behavior.
- Update the headless adapter template only enough to exercise the new public contract.

## Acceptance Criteria

- [x] Existing target-size resize tests still pass.
- [x] Pointer resize can grow a node from the bottom-right control using pointer delta.
- [x] Pointer resize from left/top controls adjusts node position as well as size.
- [x] Keep-aspect-ratio behavior is covered for diagonal and single-axis controls.
- [x] Min/max constraints clamp pointer-derived sizes.
- [x] `NodeExtent::Rect` clamps pointer-derived geometry.
- [x] `NodeExtent::Parent` clamps to a parent `Group` rect when `expand_parent` is false.
- [x] Invalid, hidden, missing-size, or no-op pointer resize inputs produce no graph mutation.
- [x] Adapter-facing behavior has conformance coverage and public-surface coverage where new public
  types or methods are exposed.
- [x] `cargo fmt --check`, focused `cargo nextest` runtime resize/conformance gates, and the
  headless adapter template gate pass.

## Notes

- Confirmed model boundary: Jellyflow nodes reference `parent: Option<GroupId>`, and groups own
  canvas-space rects. XyFlow's node-as-parent child correction does not map directly to the current
  Jellyflow model and should be treated as a follow-up unless the scope is explicitly widened.
- Out of scope for the first slice: DOM/d3 pointer integration, visual resize handles, React
  lifecycle callbacks, renderer smoke tests, node-as-parent child correction, and parent expansion
  mutations.
