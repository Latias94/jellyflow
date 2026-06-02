# Pointer Resize Session Parity Design

## Design Direction

Implement a pure, renderer-neutral pointer resize planner in `jellyflow-runtime::runtime::resize`.
The planner should derive the same graph mutation shape as the existing target-size planner:

1. compute target node geometry,
2. reject invalid/no-op cases,
3. emit `GraphOp::SetNodePos` when position changes,
4. emit `GraphOp::SetNodeSize` when size changes,
5. wrap the operations in the existing resize transaction shape.

The existing target-size API remains the compatibility floor. Pointer resize should either share the
final plan-building path or produce the same `NodeResizePlan` structure.

## Public Contract

The implementation should add a compact adapter-facing request type with these concepts:

- target `NodeId`,
- start pointer in canvas coordinates,
- current pointer in canvas coordinates,
- `NodeResizeDirection`,
- optional `NodeResizeConstraints`,
- `keep_aspect_ratio`,
- optional resize axis filter matching XyFlow's horizontal/vertical resize direction behavior.

If a new public type or store method is exposed, add public-surface coverage and crate-root
re-exports intentionally.

Adapters remain responsible for:

- pointer capture,
- DOM or platform event handling,
- transform and zoom conversion into canvas coordinates,
- snap-grid conversion before calling runtime,
- visual handle placement,
- lifecycle callback dispatch.

## Geometry Algorithm

Port the compatible parts of XyFlow's `getDimensionsAfterResize` into Rust:

- derive signed pointer deltas from the control direction,
- compute width/height changes from horizontal/vertical control axes,
- apply min/max constraints,
- account for node origin when converting size changes into position changes,
- clamp against `NodeExtent::Rect` and resolved `NodeExtent::Parent`,
- reconcile aspect ratio for diagonal and single-axis controls,
- return normalized finite position and size.

Use current Jellyflow geometry types (`CanvasPoint`, `CanvasSize`, `CanvasRect`) and keep helper
visibility narrow unless adapters need direct access.

## Extent Semantics

Reuse the drag behavior as the model:

- `NodeExtent::Rect` clamps against the given canvas-space rect.
- `NodeExtent::Parent` clamps against `graph.groups[parent].rect` when the node has a parent group
  and effective `expand_parent` is false.
- `NodeExtent::Parent` does not clamp when effective `expand_parent` is true.

Do not implement XyFlow node-as-parent child correction in this task. Jellyflow's current containment
model is group-based, so child correction needs a separate model decision.

## Conformance

Add a conformance action for pointer resize rather than overloading target-size resize fixtures. The
runner should call the store-level pointer resize entry point and then existing trace/assert helpers
should verify graph state.

The headless adapter template should include a small pointer-resize scenario that proves external
consumers can call the new API without renderer dependencies.

## Risks

- Pointer resize needs a known starting size. If `node.size` is missing or non-finite, reject the
  pointer resize in this slice instead of guessing measured geometry inside runtime.
- XyFlow pointer snapping happens before resize math. Jellyflow runtime should accept already
  snapped canvas positions; adding snap-grid policy to this task would widen adapter boundaries.
- Lifecycle callbacks are intentionally left out of the first slice. Adding them later should use
  `runtime::xyflow` or explicit conformance vocabulary, not generic resize internals.
