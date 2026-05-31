# jellyflow-runtime

`jellyflow-runtime` builds on `jellyflow-core` with the headless runtime layer:

- editor and view-state payloads;
- persistence file payloads without owning a project directory policy;
- effective interaction policy resolution under `runtime::policy`;
- validation rules and diagnostics, including connect/reconnect/delete planners;
- schema/profile pipeline hooks;
- undo/redo store dispatch;
- XyFlow-style node/edge change projections under `runtime::xyflow`;
- fit-view math that uses Jellyflow canvas geometry.

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
