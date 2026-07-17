# jellyflow-open-gpui

`jellyflow-open-gpui` is Jellyflow's retained Open GPUI adapter. It connects one headless
`ViewSession` to ordinary Open GPUI `Entity<V: Render>` node components without introducing a
portable widget tree or a cross-framework node UI DSL.

## Adapter-owned pieces

- `OpenGpuiNodeRegistry` stores sealed renderer-key registrations and mounts retained entities.
- `OpenGpuiSceneHost` reconciles frames, parks and evicts inactive instances, releases retired
  entities, publishes native layer order, and submits one ordered feedback log per frame.
- `OpenGpuiNodeBinding` exposes an owned frame snapshot to a native node. A binding read enqueues a
  revision-checked graph command into the host's ordered frame journal and returns a one-shot ticket
  that resolves from the real feedback receipt; it never mutates the runtime synchronously.
  `port_handle` creates a semantic wrapper only for a real graph `PortId`.
- `OpenGpuiPortHandle` measures a native element after layout and can add pointer, keyboard, and
  accessibility activation while leaving the child as an ordinary Open GPUI element.
- `OpenGpuiNodeInteractionGuard` shields controls, menus, scroll areas, and overlays from canvas
  gestures. Focus, IME, popup state, and application services remain in the node component.
- The scene host's internal connection coordinator manages generation-bound drag, search,
  reconnect, cancellation, freshness checks, and revisioned command creation. The host performs
  the graph query and dispatches the returned command.
- Draft and activity modules model dirty composition, conflict resolution, focus/capture leases,
  culling, and terminal retirement without persisting framework state in the graph.

## Register a native node

```rust,ignore
let mut registry = OpenGpuiNodeRegistry::builder();
registry.register("workflow.llm", |binding, window, cx| {
    cx.new(|cx| MyLlmNode::new(binding, cx))
});
let registry = registry.seal()?;
let host = OpenGpuiSceneHost::new(view, registry, viewport);
```

`MyLlmNode` is free to compose text inputs, selects, scroll views, menus, async services, and
product-specific state. It only wraps actual graph ports with `binding.port_handle(port_id)` and
reports those handles through the native element tree.

## Lifecycle and fallback

Renderer registrations are validated and sealed before a view is attached. A whitespace-only key
makes `seal` return an aggregate configuration error. Duplicate non-empty keys are isolated as
invalid entries while the remaining registry still seals. Missing renderers, isolated duplicates,
and factories that fail while mounting produce an adapter-local diagnostic fallback while the
runtime's opaque node data and real ports remain intact. A retained instance is identified by view,
node, renderer, and incarnation. Normal culling may park an eligible instance; renderer replacement,
node retirement, document replacement, and terminal view closure revoke leases, focus, capture,
overlays, subscriptions, and geometry.

The adapter keeps a bounded cache (256 eligible parked entries and 64 protected activity owners by
default). Focused, composing, captured, overlay, and dirty-draft nodes are protected or explicitly
externalized/cancelled before eviction.

Applications that preserve dirty product fields across protected-capacity overflow own an
`ExternalizedDraftStore<T>`, bind it to each `ViewSession`, and pass clones into the corresponding
node factories. Restore is keyed by the exact view, node, renderer, incarnation, and field identity;
it returns `DraftRecovery` diagnostics without recreating IME ownership or replaying an interrupted
graph command. Node reincarnation and terminal view closure remove retired snapshots automatically.

At zoom 1 or above, nodes render their ordinary native component. Below zoom 1, an unprotected node
uses an adapter-owned overview proxy that still measures every real graph port. A focus, IME,
pointer-capture, overlay, or draft lease keeps the native node at readable 1:1 pixels until that
ownership ends. Product components do not receive zoom and never maintain a second scaled widget
tree.

## Product example and tests

The retained workflow and typed shader fixtures live in
`examples/canvas-jellyflow`; this crate deliberately contains no product gallery. Run the adapter
tests with:

```sh
cargo nextest run -p jellyflow-open-gpui --cargo-quiet
```

Open GPUI 0.2's test platform does not expose captured AccessKit tree updates or an action-injection
hook. The adapter tests verify the production semantic element builders through
`Element::write_a11y_info` and cover keyboard activation, while platform `TreeUpdate` and
`ActionRequest` integration awaits upstream test-support coverage.
