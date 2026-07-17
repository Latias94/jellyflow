# jellyflow-core

`jellyflow-core` contains the portable graph document model for Jellyflow:

- stable graph, node, port, edge, symbol, group, and sticky-note IDs;
- canonical in-memory graph facts with persistence-owned versioned DTOs;
- first-class binding records for graph-local objects and opaque host-owned source anchors;
- type descriptors and compatibility checks;
- interaction-policy value types;
- renderer-neutral mutation footprints for incremental consumers.

Authoritative mutation, reversible operation journals, and history belong to
`jellyflow-runtime`. `Graph` intentionally has no raw Serde representation; applications persist
it through the runtime's versioned graph-file API.

The crate is headless by contract. It must not depend on Fret, GPU, renderer, platform, or windowing
crates.

```rust
use jellyflow_core::{Graph, GraphId, NodeId};

let graph = Graph::new(GraphId::new());
let node_id = NodeId::new();

assert!(!graph.nodes().contains_key(&node_id));
```

Bindings are portable document data. Jellyflow validates graph-local binding targets such as nodes,
ports, edges, groups, and sticky notes, while source anchors keep host-owned payloads opaque so
adapters can store PDF coordinates, text ranges, image regions, or annotation identifiers without
pulling source parsers into `jellyflow-core`.
