# jellyflow-runtime

`jellyflow-runtime` builds on `jellyflow-core` with the headless runtime layer:

- editor and view-state payloads;
- persistence file payloads without owning a project directory policy;
- effective interaction policy resolution under `runtime::policy`;
- validation rules and diagnostics, including connect/reconnect/delete planners;
- schema/profile pipeline hooks;
- undo/redo store dispatch;
- XyFlow-style node/edge change projections under `runtime::xyflow`;
- renderer-neutral selection-box helpers under `runtime::selection`;
- renderer-neutral node drag planning and commit helpers under `runtime::drag`;
- fit-view math that uses Jellyflow canvas geometry;
- renderer-neutral geometry under `runtime::geometry`, including handle endpoints, edge path
  commands, and numeric hit testing.
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
- `NodeGraphStore::plan_node_drag`, `NodeGraphStore::apply_node_drag`, and `runtime::drag` for
  deterministic canvas-space node dragging with selected-node co-dragging, policy filtering,
  snap-to-grid, global/per-node extents, and node-origin-aware clamping;
- `runtime::events::NodeGraphGestureEvent` node drag start/update/end payloads for adapters that
  want XyFlow-style drag lifecycle callbacks without coupling the runtime to pointer capture;
- rules-derived connect/reconnect/delete planners for graph transactions;
- `runtime::xyflow` projections for XyFlow-style node/edge changes and callbacks;
- `runtime::conformance::{ConformanceScenario, ConformanceAction, ConformanceTraceEvent,
  run_conformance_scenario}` for reusable fixture checks that record normalized graph commit, view,
  gesture, and callback traces around a real `NodeGraphStore`.

Run conformance fixtures before renderer smoke tests. They prove the adapter is translating intent
into the same runtime actions and callback ordering that Jellyflow expects. GPU, windowing,
screenshot, and pixel smoke tests should live in adapter crates such as future wgpu, egui, or Fret
integrations, where they can verify input capture, platform wiring, and rendered pixels.

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
