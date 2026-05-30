# jellyflow-core

`jellyflow-core` contains the portable graph document model for Jellyflow:

- stable graph, node, port, edge, symbol, group, and sticky-note IDs;
- serializable node graph data structures;
- type descriptors and compatibility checks;
- interaction-policy value types;
- undoable graph operations, transactions, fragments, and history.

The crate is headless by contract. It must not depend on Fret, GPU, renderer, platform, or windowing
crates.

```rust
use jellyflow_core::{Graph, GraphId, NodeId};

let graph = Graph::new(GraphId::new());
let node_id = NodeId::new();

assert!(!graph.nodes.contains_key(&node_id));
```
