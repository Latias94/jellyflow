# jellyflow-open-gpui

`jellyflow-open-gpui` is Jellyflow's retained Open GPUI adapter. It connects one headless
`ViewSession` to ordinary Open GPUI `Entity<V: Render>` node components without introducing a
portable widget tree or a cross-framework node UI DSL.

## Adapter-owned pieces

- `OpenGpuiNodeRegistry` stores sealed renderer-key registrations and mounts retained entities.
- `OpenGpuiSceneHost` reconciles frames, parks and evicts inactive instances, releases retired
  entities, publishes native layer order, and submits one ordered feedback log per frame.
- `authoring::OpenGpuiNodeBinding` is the long-lived capability for one exact retained mount. It
  owns activity, drag, overlay, content measurement, and layout-invalidation capabilities, but it
  does not expose a mutable or independently refreshable graph view.
- `OpenGpuiNodeBinding::authoring` opens one owned, immutable
  `authoring::OpenGpuiNodeAuthoring` for the current graph revision. Its ordered ports pair graph
  facts, constant-work connection summaries, and an `authoring::OpenGpuiPortHandle` from that same revision. Consuming the view
  with `enqueue(command, cx)` validates the mount, incarnation, and exact command version before
  the command can enter the host's ordered journal. The ticket resolves from real frame feedback;
  graph mutation is never synchronous and stale commands are never silently retried.
- `authoring::OpenGpuiPortHandle` measures a native element after layout and can add pointer, keyboard, and
  accessibility activation while leaving the child as an ordinary Open GPUI element.
- Ordinary Open GPUI controls and scroll areas are safe inside nodes without wrappers.
  `OpenGpuiOverlayInteractionBoundary` owns popup hit-testing outside node bounds. Focus, IME,
  popup state, and application services remain in the node component.
- The scene host's internal connection coordinator manages generation-bound drag, search,
  reconnect, cancellation, freshness checks, and version-checked command creation. The host performs
  the graph query and dispatches the returned command.
- Retained-field and activity owners model dirty composition, conflict resolution, focus/capture
  leases, culling, and terminal retirement without persisting framework state in the graph.

## Register a native node

```rust,ignore
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;

let mut registry = OpenGpuiNodeRegistry::builder();
registry.register("workflow.llm", |binding, window, cx| {
    cx.new(|cx| MyLlmNode::new(binding, cx))
});
let registry = registry.seal()?;
let host = OpenGpuiSceneHost::new(view, registry, viewport);
```

`MyLlmNode` is free to compose text inputs, selects, scroll views, menus, async services, and
product-specific state. The registry accepts any `Entity<V>` where `V: Render + 'static`, so a node
is an ordinary retained Open GPUI component tree rather than a Jellyflow widget DSL. Applications
still initialize the themes, globals, and services required by their chosen Open GPUI components.

## Authoring facts, ports, and commands

Open a fresh authoring view inside a render pass or input callback and do not retain it as component
state. Every item returned by `ports()` is a real graph port in node order. A dynamic marker only
exists after the corresponding `Port` has entered the graph and a newer authoring view observes it.

```rust,ignore
let binding = self.binding.read(cx);
let authoring = binding.authoring()?;
let input = authoring.port(self.input_port).ok_or(NodeUiError::MissingInput)?;
let marker = input.handle().clone().interactive(my_port_marker());
let content = binding.measure_content(
    div()
        .child(binding.drag_handle().wrap(my_header()))
        .child(my_application_controls())
        .child(marker),
);
```

Use `measure(child)` for a non-interactive semantic port and `interactive(child)` for host-owned
pointer, keyboard, and accessibility behavior. Only use `interactive_with_observer` when the
application actually needs to observe the host-reduced port event; an empty observer is unnecessary.

Build an application command from one authoring view and consume that same view to enqueue it:

```rust,ignore
let authoring = self.binding.read(cx).authoring()?;
let port = authoring
    .port(self.output_port)
    .ok_or(NodeUiError::MissingOutput)?
    .facts()
    .clone();
let command = GraphCommand::topology(
    authoring.version(),
    TopologyEdit::new().replace_node_ports(
        authoring.node_id(),
        [TopologyPort::new(self.output_port, port)],
        IncidentEdgePolicy::Reject,
    ),
);
let ticket = authoring.enqueue(command, cx)?;
```

Opening a later authoring view does not update an older one. Enqueue rejects a retired mount, a
reincarnated node, a terminally closed view, a mismatched command version, or a graph revision that
changed before journal admission. Queueing two commands from the same committed revision is allowed;
normal runtime ordering may apply the first and return a conflict for the second.

The binding remains the lifecycle boundary. Keep an `activity::OpenGpuiActivityLease` while the
component owns focus, IME composition, a popup, an async task, or another retained resource. A
component that captures the native pointer must use
`binding.activity().capture_pointer(hitbox, window)` and keep the returned exact-capture lease until
it finishes. Open GPUI assigns a replacement `HitboxId` during prepaint, so an active component must
call `lease.rebind(replacement_hitbox, window)` before registering the replacement listeners. A
`false` result means capture ownership was lost and the component must stop without reclaiming it.
Normal completion must call `lease.release(window)`; terminal host cleanup remains the fallback and
releases only the registered hitbox.
Wrap a popup rendered outside node bounds with `binding.overlay_boundary()`. After application state
changes intrinsic content geometry, update the binding entity and call
`binding.invalidate_layout(cx)` from that binding context. The request is mount-scoped and cannot
invalidate a replacement incarnation:

```rust,ignore
self.binding.update(cx, |binding, cx| binding.invalidate_layout(cx))??;
```

## Scale and size authority

The default `SemanticLod` policy uses hysteresis: detail enters overview at zoom 0.85 and overview
returns to detail at zoom 1.0. Detail stays at native logical-pixel metrics because Open GPUI 0.2
cannot transform an arbitrary retained subtree coherently across layout, paint, hitboxes, focus,
IME, and overlays. Overview owns canonical projected node and port geometry outside the detail
band. A focused, composing, pointer-capturing, or overlay-owning node may also render a fixed-pixel
interaction island; that island cannot publish graph geometry, arm node or connection gestures, or
expose semantic graph ports. Graph-interaction and draft leases protect retention without creating
an interaction island.

An application can replace the generic semantic-LOD overview with one ordinary Open GPUI entity for
a renderer key. The detail and overview entities share the same binding, node instance, mount
lifetime, host selection chrome, and real-port capabilities. This remains an adapter-local native
factory, not a renderer-neutral UI description:

```rust,ignore
registry.register_with_overview(
    "shader.multiply",
    |binding, _window, cx| cx.new(|_| MultiplyNode::new(binding)),
    |mount, _window, cx| cx.new(|_| MultiplyOverview::new(mount)),
);
```

An overview component requests `mount.port_handle(port_id, cx)` during each render and wraps its
native marker with `measure` or `interactive`. That handle shares the underlying semantic capability
with detail while publishing overview geometry; requesting it again observes dynamic port topology
and presentation changes. The factory may return either `Entity<V: Render>` or `AnyView`. It is
created at most once per retained mount, survives zoom transitions and parking, and is released with
that mount. If no overview factory is registered, or if the factory returns an error or panics while
mounting, the existing generic overview is used. A healthy detail entity is never replaced by an
overview failure. `ComponentManaged` registrations do not create an unused overview entity.

`ComponentManaged` keeps the registered renderer in detail and gives it full responsibility for
continuous native metrics. The component can read
`binding.authoring()?.viewport_transform()` from its coherent authoring view; the host does not
synthesize a subtree transform.

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
stable 1:1 samples are required before the host queues a version-checked geometry command. One
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
node retirement, document replacement, and terminal view closure revoke leases, focus, registered
exact capture, overlays, subscriptions, and geometry. The first render also installs a window-scoped
lifecycle bridge, so terminal cleanup does not depend on another render or on releasing the final
`Entity` handle after the host is removed from its parent tree.

The adapter keeps a bounded cache (256 eligible parked entries and 64 protected activity owners by
default). Focused, composing, captured, overlay, and dirty retained-field nodes are protected or
explicitly externalized/cancelled before eviction.

Applications that preserve dirty product fields across protected-capacity overflow own a shared
`RetainedFieldStore<T>` and pass clones into their node factories. Each mounted `RetainedField<T>`
binds that store to its weak view capability automatically and owns edit activity, ticket
settlement, externalization, and restoration. Restore is keyed by the exact view, node, renderer,
incarnation, and field identity; it reports `RetainedFieldRecovery` without recreating IME ownership
or replaying an interrupted graph command. Node reincarnation and terminal view closure remove
retired snapshots automatically.

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
