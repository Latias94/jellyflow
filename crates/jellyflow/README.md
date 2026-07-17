# jellyflow
`jellyflow` is the thin public facade for Jellyflow's headless graph engine. It re-exports the
portable graph model, optional layout engines, and renderer-neutral runtime under stable module
names:

- `jellyflow::core` for graph facts, IDs, ports, edges, validation, and type descriptors;
- `jellyflow::layout` for optional headless layout engines;
- `jellyflow::runtime` for `GraphRuntime`, `ViewSession`, revisioned commands, authoritative
  history, persistence, connection facts, frames, and geometry feedback.

The intended adapter boundary is:

```text
GraphRuntime -> ViewSession -> native framework
```

The facade does not define a node widget DSL. Open GPUI node internals belong to
`jellyflow-open-gpui` and the application that registers native `Entity<V: Render>` components;
egui is provided as a small protocol witness in `jellyflow-egui`.

## Quick start

```rust
use jellyflow::prelude::*;

let runtime = GraphRuntime::builder(NodeCatalog::new()).build(Graph::new(GraphId::new()))?;
let view = runtime.attach_view(ViewSessionOptions::default())?;
assert_eq!(view.graph_snapshot()?.revision(), 0);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Use `NodeCatalog` to describe creation metadata and real ports, then let the selected native
adapter render the node's internal controls. Dynamic UI changes must become revision-checked
`GraphCommand` topology edits; adapter-local visual handles are never graph endpoints.

The crate intentionally has no dependency on a window system, renderer, or UI framework.
