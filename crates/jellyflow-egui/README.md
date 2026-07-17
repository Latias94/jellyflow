# jellyflow-egui

`jellyflow-egui` is Jellyflow's narrow immediate-mode protocol witness. It proves that the
renderer-neutral `GraphRuntime` and `ViewSession` contract can drive egui without introducing a
shared widget DSL or a cross-framework renderer trait.

`EguiNativeFrameHost` materializes one view frame, invokes application-owned egui rendering for
each admitted node, records graph commands in native event order, measures the node root and all
real ports, and submits the feedback log once. Egui widget state stays in egui and is retired by
the full Jellyflow node-instance identity when a node is replaced, removed, or its view closes.

Applications that dispatch renderers dynamically use `show_with_fallback`: returning `None` keeps
unknown-kind handling inside the adapter, emits a structured diagnostic, and renders and measures
all real ports without reading or rewriting the node's opaque payload.

This crate intentionally does not ship product galleries, schema-driven controls, renderer
catalogs, inspectors, or sample applications. Its tests use adapter-local protocol tracers and
compare graph/protocol outcomes rather than widget trees or pixel geometry. Open GPUI is the
product-tier native adapter; egui remains a compact conformance implementation for adapter authors.

Run its protocol witness tests with:

```sh
cargo nextest run -p jellyflow-egui
```

The crate depends directly on `egui`; `jellyflow-core`, `jellyflow-layout`, and `jellyflow-runtime`
remain UI-framework-free.
