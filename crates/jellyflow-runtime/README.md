# jellyflow-runtime

`jellyflow-runtime` builds on `jellyflow-core` with the headless runtime layer:

- editor and view-state payloads;
- persistence file payloads without owning a project directory policy;
- effective interaction policy resolution under `runtime::policy`;
- validation rules and diagnostics, including connect/reconnect/delete planners;
- schema/profile pipeline hooks;
- undo/redo store dispatch;
- XyFlow-style node/edge change projections and ordered adapter-array apply helpers under
  `runtime::xyflow`;
- renderer-neutral selection-box helpers under `runtime::selection`;
- renderer-neutral node drag planning, parent expansion, and commit helpers under `runtime::drag`;
- renderer-neutral node resize planning, parent expansion, and commit helpers under
  `runtime::resize`;
- renderer-neutral viewport pan/zoom helpers under `runtime::viewport`;
- renderer-neutral viewport animation and double-click zoom planning under `runtime::viewport`;
- renderer-neutral viewport pan inertia planning under `runtime::viewport`;
- renderer-neutral auto-pan frame helpers under `runtime::auto_pan`;
- renderer-neutral store-level rendering reads through `NodeGraphStore::rendering_query` and
  `runtime::rendering::RenderingQueryResult`;
- renderer-neutral delete selection planning under `runtime::delete` and key-bound routing under
  `runtime::keyboard`;
- fit-view math that uses Jellyflow canvas geometry;
- renderer-neutral geometry under `runtime::geometry`, including handle endpoints, edge path
  commands, and numeric hit testing;
- reusable headless conformance fixtures and a runner under `runtime::conformance`.

The crate stays UI-agnostic. Fret-specific conversions, widgets, rendering, and event binding remain
adapter responsibilities.

```rust
use jellyflow_core::{Graph, GraphId};
use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow_runtime::NodeGraphStore;

let store = NodeGraphStore::new(
    Graph::new(GraphId::new()),
    NodeGraphViewState::default(),
    NodeGraphEditorConfig::default(),
);

assert_eq!(store.graph().nodes.len(), 0);
```

## Headless Interaction Contracts

Renderer adapters should translate pointer and keyboard input into Jellyflow runtime calls, then
validate behavior before rendering. The runtime crate supports that split with:

- `NodeGraphStore::apply_selection_box` and `runtime::selection::compute_selection_box` for
  deterministic canvas-space selection;
- `NodeGraphStore::plan_delete_selection`, `NodeGraphStore::apply_delete_selection`,
  `NodeGraphStore::apply_delete_selection_for_key`, and `runtime::keyboard::KeyboardIntent` for
  deterministic selected node/edge deletion through effective policy, configured delete keys,
  cascaded connected-edge deletion, normal graph transactions, selection cleanup, and
  adapter-owned pre-delete `Accept`/`Veto`/`Replace` decisions;
- `runtime::connection::{resolve_connection_target_from_handles, ConnectionTargetCandidate}` for
  resolving adapter-provided handle geometry and connectability into XyFlow-style target feedback
  without owning DOM hit testing;
- `runtime::gesture::{PointerSessionClaimInput, PointerSessionClaimOutcome}` plus
  `NodeGraphStore::resolve_pointer_session_claim` for deterministic pointer ownership arbitration
  with stable rejection reasons, and `NodeGraphStore::{apply_node_drag_session,
  apply_connect_edge_session, apply_viewport_drag_pan_session}` for ordinary adapter gesture
  lifecycles through store commits and gesture events;
- `NodeGraphStore::plan_node_drag`, `NodeGraphStore::apply_node_drag`, and `runtime::drag` for
  deterministic canvas-space node dragging with selected-node co-dragging, policy filtering,
  snap-to-grid, global/per-node extents, node-origin-aware clamping, and parent group expansion;
- `NodeGraphStore::plan_node_resize`, `NodeGraphStore::apply_node_resize`, and `runtime::resize`
  for deterministic target-size node resizing with min/max bounds, XyFlow-style control directions,
  node-origin-aware position updates for left/top controls, normal graph transactions, and
  pointer-resize session lifecycle helpers;
- `runtime::viewport::{ViewportTransform, ViewportPanRequest, ViewportZoomRequest}` plus
  `NodeGraphStore::apply_viewport_pan` and `NodeGraphStore::apply_viewport_zoom` for deterministic
  drag-pan and zoom-around-pointer state changes;
- `runtime::viewport::{ViewportAnimationRequest, ViewportAnimationOptions,
  ViewportAnimationPlan, ViewportAnimationFrame, ViewportDoubleClickZoomInput}` plus
  `runtime::viewport::resolve_viewport_double_click_zoom` for deterministic animation sampling and
  normalized double-click zoom planning without runtime-owned timers or raw platform event
  detection;
- `runtime::viewport::{ViewportPanInertiaRequest, ViewportPanInertiaPlan,
  ViewportPanInertiaFrame}` plus `runtime::viewport::plan_viewport_pan_inertia` for deterministic
  pan inertia sampling from adapter-provided logical screen px/s release velocity and
  `NodeGraphPanInertiaTuning`;
- `runtime::auto_pan::{AutoPanRequest, SelectionAutoPanRequest, AutoPanPlan}` plus
  `NodeGraphStore::{apply_auto_pan, apply_selection_auto_pan}` for deterministic edge-proximity
  auto-pan frames that feed the normal viewport publication path;
- `NodeGraphStore::layout_facts_query` for adapter-facing report-once/read-many layout facts after
  measurement publication. It returns the current layout-facts revision, renderer-facing
  `rendering_query` result, visible edge endpoints, and connection target candidates; selector
  subscriptions can track `layout_facts_revision` for redraw/re-query decisions. Narrow
  `NodeGraphStore::rendering_query` and `NodeGraphStore::{visible_node_ids,
  visible_node_render_order, visible_edge_ids, visible_edge_render_order}` helpers remain available
  for callers that only need paint order or visibility lists;
- `runtime::events::NodeGraphGestureEvent` node drag start/update/end payloads for adapters that
  want XyFlow-style drag lifecycle callbacks without coupling the runtime to pointer capture;
- `runtime::events::NodeGraphGestureEvent` viewport move start/update/end payloads for adapters
  that want XyFlow-style `onMoveStart`, `onMove`, and `onMoveEnd` callbacks around pan/zoom
  gestures;
- rules-derived connect/reconnect/delete planners for graph transactions;
- `runtime::xyflow` projections for XyFlow-style node/edge changes, callbacks, and exact
  adapter-owned ordered-array apply helpers;
- `runtime::conformance::{ConformanceScenario, ConformanceSuite, ConformanceFixtureDirectory,
  ConformanceAction, ConformanceTraceEvent, run_conformance_scenario, run_conformance_suite}` for
  reusable fixture checks, fixture discovery, and explicit golden approval updates around a real
  `NodeGraphStore`.

Run conformance fixture suites before renderer smoke tests. They prove the adapter is translating
intent into the same runtime actions and callback ordering that Jellyflow expects, and they return
aggregate reports that separate trace mismatches from scenario execution errors. Suites can be saved
and loaded as pretty JSON files through `ConformanceSuite::save_json`, `load_json`, and
`load_json_if_exists`; directories can be discovered recursively through
`ConformanceFixtureDirectory::load_json` and `load_json_if_exists`, so adapters and agents can keep
durable golden fixture assets in their own repos. Approval is explicit: `approve_actual_traces`
returns an updated suite/report, while file and directory `approve_actual_traces_to_json` helpers
write back only when every scenario executes without errors. GPU, windowing, screenshot, and pixel
smoke tests should live in adapter crates such as future wgpu, egui, or Fret integrations, where
they can verify input capture, platform wiring, and rendered pixels.

Drag parent expansion is runtime-owned: a child with effective `expand_parent = true` can expand
its parent group rect through `GraphOp::SetGroupRect`, while `NodeExtent::Parent` with
`expand_parent = false` still clamps to the current parent group rect. Jellyflow stores node
positions in canvas space, so left/top group expansion does not add sibling compensation ops. Raw
pointer capture, drag handles, resize handles, renderer-specific grouping UI, screenshots, and
pixels remain adapter responsibilities.

Resize planning is runtime-owned: adapters provide normalized canvas-space `NodeResizeRequest` or
`NodePointerResizeRequest` values, and the runtime produces `node resize` transactions from existing
`GraphOp::SetNodeSize`, `GraphOp::SetNodePos` for left/top controls, and `GraphOp::SetGroupRect`
when `expand_parent = true` expands a parent group. `NodeExtent::Parent` with
`expand_parent = false` still clamps to the current parent group rect. Adapters still own resize
handle UI, raw pointer capture, cursor policy, renderer feedback, and pixels. Exact XyFlow
node-owned child containment is intentionally not modeled as a renderer or DOM dependency in
`jellyflow-runtime`.

Delete selection planning is runtime-owned: adapters maintain view-state selection and translate
platform keyboard input into direct delete calls or `KeyboardIntent`. The runtime resolves the
configured delete key, effective `deletable` policy, selected nodes/edges, cascaded connected-edge
deletion, `delete selection` transactions, XyFlow-style callback projections, and stale selection
cleanup. For XyFlow-style `onBeforeDelete`, adapters call `prepare_delete_selection` or
`prepare_delete_selection_for_key`, await their own hook/UI, then call `apply_pre_delete_resolution`
with `PreDeleteResolution::Accept`, `Veto`, or `Replace`. Adapters still own raw key capture,
focus/input suppression, confirmation dialogs, async scheduling, renderer feedback, screenshots,
and pixels.

`ConformanceAction::dispatch_transaction` is intentionally kept as a low-level graph-operation
fixture escape hatch; adapter feel fixtures should prefer interaction-specific actions such as
node drag, node resize, connect/reconnect, delete, viewport gestures, viewport animation frames,
and double-click zoom plan or rejection assertions. Visible-node fixtures should use
`ConformanceAction::assert_visible_node_ids`, which asserts `NodeGraphStore::visible_node_ids`
without producing renderer traces. Pre-render paint-order fixtures should use
`ConformanceAction::assert_visible_node_render_order`, which asserts
`NodeGraphStore::visible_node_render_order` without producing renderer traces. Visible-edge
fixtures should use the matching visible edge assertions, while adapters that need all lists should
read `NodeGraphStore::rendering_query` once. Pan inertia fixtures
should use the sampled-frame actions and rejection assertion so adapters can prove release-momentum
traces without moving frame loops into runtime. Parent expansion fixtures should use
`ConformanceAction::apply_node_drag`, which exercises the same runtime interaction boundary and
records `set_group_rect` graph-commit traces when parent expansion occurs. Resize fixtures should
use `ConformanceAction::apply_node_resize` or `apply_node_pointer_resize`, which exercises the same
runtime interaction boundary and records `set_node_size`, `set_node_pos` plus `set_node_size`, and
`set_group_rect` graph-commit traces when parent expansion occurs. Delete fixtures should use
`ConformanceAction::apply_delete_selection` or `apply_delete_selection_for_key`, which records
`remove_node` or `remove_edge` graph commits, XyFlow-style delete/disconnect callbacks, and
selection cleanup traces.

The runtime crate also includes a thin renderer-free example harness for agents and CI:

```text
cargo run -p jellyflow-runtime --example conformance_harness -- check <fixture-dir>
cargo run -p jellyflow-runtime --example conformance_harness -- approve <fixture-dir>
```

For a copyable external adapter skeleton, start with the non-workspace headless template:

```text
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

Viewport conformance is also headless. Runtime tests cover:

- screen-delta pan conversion at the current zoom;
- anchored zoom that keeps the pointer's canvas coordinate stable;
- auto-pan conversion from pointer-edge proximity and elapsed frame time into viewport pan frames;
- `NodeGraphStore` view-state publication for viewport intent;
- viewport move gesture callback ordering;
- fixture-runner traces for pan, zoom, auto-pan, view changes, gestures, and XyFlow-style callbacks;
- conformance assertions for viewport animation frame sampling, sampled-frame viewport traces, and
  double-click zoom plan or rejection outcomes.
- conformance assertions for pan inertia frame sampling, sampled-frame viewport traces, and rejected
  below-threshold inertia plans.
- conformance assertions for visible node ids, visible edge ids, and render order before
  renderer-specific draw batching.

Adapters still own raw wheel delta normalization, pinch detection, pointer capture, cursor policy,
raw double-click detection, release velocity estimation, frame scheduling, animation and inertia
cancellation policy, sampled-frame commits, edge routing and draw batching, resize handles, window
event loops, screenshots, and pixel assertions. For selection workflows, adapters own the
screen-space selection rectangle and pointer/session ownership, then call `SelectionAutoPanRequest`
for the shared edge-proximity viewport motion. Low-level geometry helpers remain available for
custom routing and hit testing; adapters that need store-derived endpoints after reporting
measurements should prefer `NodeGraphStore::layout_facts_query`.

```rust
use jellyflow_core::{CanvasPoint, CanvasRect, CanvasSize};
use jellyflow_runtime::runtime::geometry::{
    edge_path_contains_point, edge_position, straight_edge_path, EdgeEndpointInput,
    EdgeHitTestOptions, HandlePosition,
};

let endpoints = edge_position(
    EdgeEndpointInput {
        node_rect: CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize { width: 120.0, height: 80.0 },
        },
        handle: None,
        fallback_position: HandlePosition::Right,
    },
    EdgeEndpointInput {
        node_rect: CanvasRect {
            origin: CanvasPoint { x: 240.0, y: 40.0 },
            size: CanvasSize { width: 120.0, height: 80.0 },
        },
        handle: None,
        fallback_position: HandlePosition::Left,
    },
)
.expect("edge endpoints");

let path = straight_edge_path(endpoints.source, endpoints.target).expect("path");
assert!(edge_path_contains_point(
    &path,
    CanvasPoint { x: 180.0, y: 40.0 },
    EdgeHitTestOptions::default(),
));
```
