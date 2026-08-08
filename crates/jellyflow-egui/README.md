# jellyflow-egui

`jellyflow-egui` is Jellyflow's narrow immediate-mode protocol witness. It proves that the
renderer-neutral `GraphRuntime` and `ViewSession` contract can drive egui without introducing a
shared widget DSL or a cross-framework renderer trait.

`EguiNativeFrameHost` materializes one view frame, invokes application-owned egui rendering for
each admitted node, records graph commands in native event order, measures the node root and all
real ports, and submits the feedback log once. Egui widget state stays in egui and is retired by
the full Jellyflow node-instance identity when a node is replaced, removed, or its view closes.
Application renderers must use a bounded set of stable state-ID salts per node lifetime; registered
state intentionally survives temporarily hidden rows until that node instance is hard-retired.

`EguiCanvasInteraction` is the matching input bridge. Begin it with the current scene rectangle,
pass only explicit graph-interactive `egui::Response` values to `handle_response`, and end it after
all responses have been rendered. The bridge normalizes raw press/move/release events, owns the
active egui `Id`, and resolves bounded runtime geometry queries. The attached interaction session
commits view-local selection and viewport state to the same `ViewSession`. Missing responses,
stolen drag ownership, escape,
pointer loss, owner retirement, and view close all terminate native ownership explicitly.

```rust,ignore
interaction.begin_frame(ui.ctx(), ui.max_rect())?;
interaction.handle_response(&canvas_keyboard_scope, CanvasPressTarget::Pan)?;
interaction.handle_response(&drag_handle, node_drag_target)?;
interaction.handle_response(&port_handle, connection_target)?;
let output = interaction.end_frame(ui.ctx())?;
```

The focused graph response is also the keyboard authority boundary: Delete and Backspace submit one
policy-filtered selection deletion, while native controls and focused text editors retain those keys.

`output` retains only application work: create-from-wire search requests and fresh graph mutations.
The application must bind graph mutations to a fresh snapshot, preserve its command ordering, and
return the resulting identity-keyed completion. This crate deliberately does not copy Open GPUI's command
journal, visual theme, or retained component lifecycle.

Applications that dispatch renderers dynamically use `show_with_fallback`: returning `None` keeps
unknown-kind handling inside the adapter, emits a structured diagnostic, and renders and measures
all real ports without reading or rewriting the node's opaque payload.

This crate intentionally does not ship product galleries, schema-driven controls, renderer
catalogs, inspectors, or sample applications. Its tests drive real egui pointer/key events through
the shared connection, candidate, edge-pick, node-drag, cancellation, retirement, close, and
completion traces. Open GPUI is the product-tier native adapter; egui remains a compact independent
implementation witness for adapter authors.

Run its protocol witness tests with:

```sh
cargo nextest run -p jellyflow-egui
```

The crate depends directly on `egui`, `jellyflow-core`, and `jellyflow-runtime`. It does not depend
on a layout engine or the Open GPUI adapter; the headless crates remain UI-framework-free.
