# Jellyflow

`jellyflow` is the user-friendly facade crate for the headless Jellyflow graph engine.

It re-exports the lower-level crates under stable module names:

- `jellyflow::core` for document data, IDs, graph operations, and interaction primitives;
- `jellyflow::layout` for optional headless layout engines and layout discovery metadata;
- `jellyflow::runtime` for the headless store, schema/profile pipeline, rules, conformance helpers,
  and renderer-neutral adapter contracts.

Use this crate when you want one dependency for application or adapter code. Depend on
`jellyflow-core`, `jellyflow-layout`, or `jellyflow-runtime` directly when you need a narrower
dependency boundary. For automatic layout, start with `LayoutPresetBuilder::workflow()`,
`LayoutPresetBuilder::tree()`, `LayoutPresetBuilder::mind_map()`, or
`LayoutPresetBuilder::freeform()`, then pass the built `LayoutEngineRequest` to the runtime layout
facade.

```rust
use jellyflow::prelude::*;

let store = NodeGraphStore::new(
    Graph::new(GraphId::new()),
    NodeGraphViewState::default(),
    NodeGraphEditorConfig::default(),
);

assert_eq!(store.graph().nodes().len(), 0);
```
