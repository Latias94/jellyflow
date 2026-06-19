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
cargo run -p jellyflow-egui --example custom_widget
cargo run -p jellyflow-egui --example gallery_snapshot
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
adapter consumes those regions for widget placement and anchor-aware handle placement, so complex
nodes can align ports to internal rows without changing the headless graph model.
Schemas can declare matching renderer-neutral `NodeSurfaceSlotDescriptor` values so field rows,
badges, actions, previews, and nested regions are described as semantic data rather than egui
widgets.
Use `slot` for the semantic data path and `anchor` for adapter-local placement or port binding.
Use `NodeWidgetRenderInput::region_screen_rect` when placing child widgets so clipping stays
consistent with the canvas viewport.

## Custom node widgets

Use two adapter-owned renderer traits for complex nodes:

- `RichNodeRenderer` measures the node and emits node-local interactive regions.
- `EguiNodeWidgetRenderer` draws egui child UI into those regions.

Register both implementations under the descriptor `renderer_key`:

```rust
let mut renderers = RendererCatalog::default();
renderers
    .register("review-card", NodeRendererStyle::task())
    .register_rich("review-card", ReviewCardRenderer)
    .register_widgets("review-card", ReviewCardRenderer);
```

Then point a schema at that key and anchor ports to the regions produced by the renderer:

```rust
let schema = NodeSchema::builder("demo.review_card", "Review card")
    .renderer_key("review-card")
    .default_size(CanvasSize {
        width: 246.0,
        height: 136.0,
    })
    .port(
        PortDecl::data_input("source")
            .on_left()
            .with_view_anchor("field.assignee"),
    )
    .port(
        PortDecl::data_output("approved")
            .on_right()
            .with_view_anchor("field.status"),
    )
    .surface_slot(
        NodeSurfaceSlotDescriptor::field_row("field.assignee")
            .with_label("Assignee")
            .with_anchor("field.assignee"),
    )
    .surface_slot(
        NodeSurfaceSlotDescriptor::field_row("field.status")
            .with_label("Status")
            .with_anchor("field.status"),
    )
    .build();
```

The complete example lives in `examples/custom_widget.rs`. It builds a custom review-card node with
embedded egui rows, zoom-aware content levels, and ports anchored to row regions.

## Visual review gallery

Run `cargo run -p jellyflow-egui --example gallery_snapshot` to export PNGs for every product
sample into `target/jellyflow-egui-gallery`. This is intentionally a local review aid instead of a
CI pixel gate, because GPU snapshot output can vary across platforms.

The crate intentionally depends on `eframe`; the core `jellyflow`, `jellyflow-core`,
`jellyflow-layout`, and `jellyflow-runtime` crates remain renderer-free.
