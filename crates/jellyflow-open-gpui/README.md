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
  `port_handle` creates a semantic wrapper only for a real graph `PortId`; `measure_content`
  reports a native 1:1 content root for an opt-in content size policy.
- `OpenGpuiPortHandle` measures a native element after layout and can add pointer, keyboard, and
  accessibility activation while leaving the child as an ordinary Open GPUI element.
- Ordinary Open GPUI controls and scroll areas are safe inside nodes without wrappers.
  `OpenGpuiOverlayInteractionBoundary` owns popup hit-testing outside node bounds. Focus, IME,
  popup state, and application services remain in the node component.
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

## Scale and size authority

The default `SemanticLod` policy uses hysteresis: detail enters overview at zoom 0.85 and overview
returns to detail at zoom 1.0. Detail stays at native logical-pixel metrics because Open GPUI 0.2
cannot transform an arbitrary retained subtree coherently across layout, paint, hitboxes, focus,
IME, and overlays. Overview owns canonical projected node and port geometry outside the detail
band. A focused, composing, or overlay-owning node may also render a fixed-pixel interaction island;
that island cannot publish graph geometry, arm node or connection gestures, or expose semantic graph
ports. Pointer capture, graph interaction, and draft leases protect retention but do not create an
interaction island.

`ComponentManaged` keeps the registered renderer in detail and gives it full responsibility for
continuous native metrics. The component can read `binding.read()?.viewport_transform()`; the host
does not synthesize a subtree transform.

Document size is authoritative by default. A renderer may opt into fixed-width `ContentHeight` or
bounded two-axis `Content` sizing at registration:

```rust,ignore
let policy = OpenGpuiNodeRegistrationPolicy::default()
    .with_size_policy(OpenGpuiNodeSizePolicy::ContentHeight(
        OpenGpuiContentHeightPolicy::new(96.0, 720.0)?,
    ));
registry.register_with_policy("workflow.condition", policy, |binding, _window, cx| {
    cx.new(|_| ConditionNode::new(binding))
});
```

The component wraps one naturally sized native root with `binding.measure_content(content)`. Two
stable 1:1 samples are required before the host queues a revision-checked geometry command. One
command may be pending, newer targets coalesce, and bounded cycle detection freezes a non-convergent
epoch. Call `binding.invalidate_layout(cx)` when product state changes intrinsic layout; environment
changes start a separate convergence epoch. No graph mutation occurs during render or prepaint.

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
