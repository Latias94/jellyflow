# Resize Lifecycle Callback Design

## Boundary

Resize lifecycle belongs to transient runtime gesture observation, not persisted graph state. The
implementation should follow the existing node drag boundary:

- event payloads live under `runtime::events`;
- XyFlow-compatible hook names live under `runtime::xyflow::callbacks`;
- conformance traces expose the adapter-facing contract;
- resize math and graph commits remain under `runtime::resize` and `NodeGraphStore`.

## Event Payloads

Add a new `runtime/events/node_resize.rs` module with:

- `NodeResizeStart { node, direction, pointer }`
- `NodeResizeUpdate { node, direction, pointer, position, size }`
- `NodeResizeEnd { node, direction, pointer, outcome }`
- `NodeResizeEndOutcome { Committed, Rejected, Canceled, NoOp }`

`direction` should reuse `runtime::resize::NodeResizeDirection`. `position` and `size` are
canvas-space committed geometry snapshots for the update callback. Payloads should derive
`Debug`, `Clone`, `PartialEq`, `Serialize`, and `Deserialize`; the outcome enum should also derive
`Copy` and `Eq`.

Add `NodeResizeStart`, `NodeResizeUpdate`, and `NodeResizeEnd` variants to
`NodeGraphGestureEvent`.

## Callback Projection

Extend `NodeGraphGestureCallbacks` with default no-op methods:

- `on_node_resize_start(NodeResizeStart)`
- `on_node_resize(NodeResizeUpdate)`
- `on_node_resize_end(NodeResizeEnd)`

Update gesture dispatch so each new `NodeGraphGestureEvent` variant invokes the matching callback.
Re-export the new payloads through `runtime::xyflow::callbacks::types` alongside existing drag and
viewport gesture types.

## Conformance Flow

Reuse `ConformanceAction::emit_gesture` and `ConformanceAction::apply_node_pointer_resize`.

Expected happy-path ordering:

1. emit `NodeResizeStart`
2. callback recorder records `NodeResizeStart`
3. apply pointer resize and record graph commit callbacks
4. emit `NodeResizeUpdate` with the committed geometry
5. callback recorder records `NodeResize`
6. emit `NodeResizeEnd`
7. callback recorder records `NodeResizeEnd`

This mirrors the existing node drag conformance structure and keeps lifecycle emission explicit for
headless adapters.

## Compatibility

- Existing target-size and pointer resize APIs remain unchanged.
- Existing conformance fixtures without resize lifecycle events should keep the same trace output.
- New callback trait methods are source-compatible for implementations that rely on default methods.
- Serialized conformance fixtures gain new enum variants but do not require fixture migration.

## Deferred Work

- `shouldResize` callback gating and rejected update semantics.
- Store-owned active resize sessions.
- XyFlow child correction and parent expansion during node-as-parent resize.
- UI handle components or DOM event translation.
