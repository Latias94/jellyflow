# Jellyflow Headless Adapter Template

This crate is a copyable starting point for a renderer or UI adapter. It stays outside the
Jellyflow workspace so it behaves like an external consumer.

The template runs Jellyflow's headless conformance layer before any renderer-specific smoke tests:

```text
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check <fixture-dir>
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- approve <fixture-dir>
```

Use the built-in suite as the first CI gate for adapter input translation. It covers node drag,
node resize, selection deletion, viewport pan, sampled viewport animation frames, double-click zoom
planning, and sampled viewport pan inertia before rendering. Keep renderer frame loops, animation
cancellation policy, `wgpu`, egui, Fret, screenshots, and pixel checks in adapter-specific crates or
test suites.

Custom node renderers remain adapter-owned. The template demonstrates the extension point with
`AdapterRendererRegistry`: Jellyflow schema descriptors expose a stable `renderer_key`, declared
ports, default data, and default size; the adapter maps that key to its own React, Svelte, native,
or immediate-mode renderer before calling `NodeGraphStore::apply_create_node_from_schema`.

The template starts from the same built-in node kits as Jellyflow's internal proof and sample
surfaces, then adds its own `template.note` schema on top. That keeps kit reuse visible in external
consumer tests without turning the template into a parallel schema source.

Use store subscriptions as invalidation signals, not as renderer state containers. Subscribe to
small projections such as graph and layout-facts revisions, then call
`NodeGraphStore::rendering_query` for the current viewport to get visible IDs and render order.
Keep component instances, memoization, batching, and pixel tests in the adapter repository.

Adapter responsibility checklist:

- Map each `renderer_key` to local toolkit components; do not put framework widget types in
  Jellyflow runtime/core data.
- Treat `slot` as the semantic data lookup path and `anchor` as layout or port-binding metadata.
- Report measured slot, anchor, handle, node-size, and density facts after rendering custom node
  internals.
- Invalidate and remeasure node internals after data, component state, zoom, or resize changes that
  move rows, handles, previews, or toolbars.
- Use runtime connection lifecycle, edge route, resize, viewport, and conformance APIs before adding
  renderer-specific pointer or screenshot tests.
- Keep backend workflow execution, shader compilation, database IO, synchronization, and remote
  collaboration in the host product.

When copying this template into another repository, replace the path dependencies in `Cargo.toml`
with the Jellyflow version you want to consume.
