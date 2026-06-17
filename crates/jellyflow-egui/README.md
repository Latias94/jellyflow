# jellyflow-egui

Immediate-mode egui adapter for Jellyflow node graphs.

This crate owns egui rendering, panels, pointer input, and a runnable `eframe::App`. Graph
semantics stay in the headless Jellyflow crates: schema-driven node creation, selection, drag,
resize, connect, delete, undo/redo, viewport math, layout, and rendering queries all go through
`NodeGraphStore`.

```sh
cargo run -p jellyflow-egui --example demo
cargo run -p jellyflow-egui --example workflow
cargo run -p jellyflow-egui --example automation_builder
cargo run -p jellyflow-egui --example mind_map
cargo run -p jellyflow-egui --example tree
cargo run -p jellyflow-egui --example org_chart
cargo run -p jellyflow-egui --example knowledge_board
cargo run -p jellyflow-egui --example erd
```

The demo ships with a sample gallery for workflow, automation builder, mind map, tree, org chart,
knowledge-board, and ERD-style graphs. It includes selection, marquee selection, node dragging,
handle-to-handle connections, selected node resize handles, viewport panning and zooming, keyboard
nudging, cursor feedback, layout presets, undo/redo, delete, a schema-driven palette, and a read-only
inspector.

The samples are product fixtures, not separate domain frameworks:

- Workflow and automation builder reuse schema descriptors, exec/data ports, edge labels, and
  profile-ready metadata for Dify-like LLM/tool branches and error paths.
- ERD uses the built-in `table-card` renderer to expose node-local field regions; table ports with
  `field.*` anchors are placed on those field rows, with edge-owned cardinality labels.
- Mind map and org chart stress hierarchy layout with the same node/edge graph model.
- Knowledge board stresses freeform source, claim, question, action, and output cards.

Use `JellyflowEguiBridge` when embedding the adapter into your own app. Register node schemas in a
`NodeRegistry`, map each descriptor `renderer_key` to your own `RendererCatalog` style, then let the
egui canvas call Jellyflow runtime APIs instead of mutating graph storage directly.

Rich renderers return `NodeRenderLayout` plus node-local `NodeInteractiveRegion` values. The egui
adapter consumes those regions for field-row painting and anchor-aware handle placement, so complex
nodes can align ports to internal rows without changing the headless graph model.

The crate intentionally depends on `eframe`; the core `jellyflow`, `jellyflow-core`,
`jellyflow-layout`, and `jellyflow-runtime` crates remain renderer-free.
