# jellyflow-egui

Immediate-mode egui adapter for Jellyflow node graphs.

This crate owns egui rendering, panels, pointer input, and a runnable `eframe::App`. Graph
semantics stay in the headless Jellyflow crates: schema-driven node creation, selection, drag,
resize, connect, delete, undo/redo, viewport math, layout, and rendering queries all go through
`NodeGraphStore`.

```sh
cargo run -p jellyflow-egui --example demo
```

Use `JellyflowEguiBridge` when embedding the adapter into your own app. Register node schemas in a
`NodeRegistry`, map each descriptor `renderer_key` to your own `RendererCatalog` style, then let the
egui canvas call Jellyflow runtime APIs instead of mutating graph storage directly.

The crate intentionally depends on `eframe`; the core `jellyflow`, `jellyflow-core`,
`jellyflow-layout`, and `jellyflow-runtime` crates remain renderer-free.
