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

Authoring UI follows the same boundary. Jellyflow exposes semantic descriptors for controls,
repeatable collections, actions, menus, inspectors, and blackboards; the adapter maps those facts
to its own widgets. Do not share widget instances or framework component types through runtime
schema data. A Dify-like node can project a textarea, model select, variable picker, parameter
array, run action, dropped-wire menu, and inspector target from descriptors, while egui, GPUI, or
Dioxus still render their local controls.

The template starts from the same built-in node kits as Jellyflow's internal proof and sample
surfaces, then adds its own `template.note` schema on top. That keeps kit reuse visible in external
consumer tests without turning the template into a parallel schema source.

The template also exposes an adapter capability matrix through `template_capabilities()` and the
`ConformanceSuite` report. Capability levels are intentionally conservative:

- `none`: the adapter should not claim the capability.
- `projection`: the adapter can consume or project the semantic contract, but does not prove local
  editing or real toolkit layout yet.
- `partial`: the adapter proves useful behavior for some product shapes, but not the full contract.
- `full`: conformance and adapter tests prove the capability end to end.

The template claims full support for headless measured handles, measured anchors, and dynamic
internals because those are exercised by the smoke suite. It claims projection support for
authoring descriptors and layout-pass measurement vocabulary only; real editable controls,
menus, inspectors, keyboard behavior, visual regression, and toolkit layout-pass bounds remain
adapter-specific work.

Use store subscriptions as invalidation signals, not as renderer state containers. Subscribe to
small projections such as graph and layout-facts revisions, then call
`NodeGraphStore::rendering_query` for the current viewport to get visible IDs and render order.
Keep component instances, memoization, batching, and pixel tests in the adapter repository.

Adapter responsibility checklist:

- Map each `renderer_key` to local toolkit components; do not put framework widget types in
  Jellyflow runtime/core data.
- Map `NodeControlDescriptor`, repeatable item templates, `NodeActionDescriptor`, `MenuDescriptor`,
  `InspectorDescriptor`, and `BlackboardDescriptor` to local widgets, popups, panels, and keyboard
  routing.
- Treat `slot` as the semantic data lookup path and `anchor` as layout or port-binding metadata.
- Keep repeatable item anchors based on item ids, not visible indexes, so edges and measurements
  follow logical rows after add, remove, and reorder.
- Report measured slot, anchor, handle, node-size, and density facts after rendering custom node
  internals.
- Invalidate and remeasure node internals after data, component state, zoom, or resize changes that
  move rows, handles, previews, or toolbars.
- Use runtime connection lifecycle, edge route, resize, viewport, and conformance APIs before adding
  renderer-specific pointer or screenshot tests.
- Report adapter capabilities truthfully: projection proofs must not claim full editable controls
  or full toolkit layout-pass measurement.
- Add adapter-local geometry or visual gates for rich nodes. Jellyflow's egui gate is documented in
  `docs/testing/node-ui-authoring-regression.md`; copy the matrix, but implement it with the host
  toolkit's own layout and screenshot tools.
- Keep backend workflow execution, shader compilation, database IO, synchronization, and remote
  collaboration in the host product.

When copying this template into another repository, replace the path dependencies in `Cargo.toml`
with the Jellyflow version you want to consume.
