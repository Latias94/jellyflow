# Native Adapter Authoring

Jellyflow separates graph semantics from native UI ownership. An adapter does not implement a
Jellyflow widget vocabulary. It translates one UI framework's frame, geometry, and input lifecycle
into the renderer-neutral contracts provided by `jellyflow-runtime`.

## Dependency Boundary

Choose direct dependencies according to the behavior the adapter owns:

- `jellyflow-core` provides graph identities, nodes, ports, edges, and persisted graph facts.
- `jellyflow-runtime` provides commands through `commands`, graph authority through `graph`,
  view-scoped frames and feedback through `view`, geometry and viewport facts through their owning
  modules, interaction semantics through `interaction`, and the optional `adapter::conformance`
  contract.
- `jellyflow-layout` is optional. Depend on it only when the adapter or application runs a layout
  engine.
- A native UI adapter depends on its own UI framework. Framework types must not enter the core or
  runtime crates.

Depend on these crates directly. Do not introduce a portable control DSL, shared raw-event
dispatcher, or portable theme; those would hide the lifecycle and input ownership that a real UI
framework must implement. Open GPUI and egui are reference implementations of the same semantic
boundary, not base classes for other adapters.

## Ownership Map

`GraphRuntime` is the only graph mutation authority. A `ViewSession` owns one view's viewport,
selection, derived geometry, and revisioned adapter protocol. One or more explicitly attached
`CanvasInteractionSession` values own independent gesture lifecycles and synchronously commit
view-local selection and viewport changes through their bound weak view authority. The native
adapter owns retained or immediate widget state, focus, IME, overlays, pointer capture, theme,
painting, and conversion from framework events to normalized semantic input.

An application owns product behavior such as node controls, validation, dynamic-port intent,
connection-search providers, labels, and command construction. An adapter may provide native
helpers for its framework, but those helpers must preserve real graph identities and explicit
revision checks.

## Frame Protocol

For each native frame:

1. Materialize an owned adapter frame from the target `ViewSession`.
2. Reconcile native node instances against the exact `NodeLifetimeKey` and renderer identity.
3. Render or lay out native content without mutating graph or view authority.
4. Publish complete node and real-port geometry through the frame's feedback collector.
5. Transfer at most one ordered native command batch into the frame transaction.
6. Submit the collector and batch exactly once, including explicit invalid or omitted layout
   outcomes.
7. Process command receipts and the next authoritative frame before replacing published geometry.

Never retain a graph borrow, `ViewSession` borrow, adapter frame, or feedback collector across native
frames or asynchronous task boundaries. Keep the last authoritative geometry visible while newer
feedback is incomplete or rejected. A layout failure must not discard a valid command batch. If a
collected batch cannot reach submission, settle it as a definite frame-preparation failure; if
submission began but its outcome cannot be observed, settle it as outcome-unknown. Host close,
retirement, panic unwinding, and dropped frame work must all release the batch exactly once.

Content-derived size measurements are observations, not same-frame mutations. Queue a stable,
revision-bound size intent only after render so it enters a later frame and cannot feed back into
the measurement that produced it.

## Native Nodes And Ports

A native node may contain any components supported by its UI framework. Jellyflow needs semantic
participation only at graph boundaries:

- Mount state is scoped to the exact `NodeInstanceKey`, including view, node lifetime, and renderer.
- Every connectable endpoint corresponds to a current graph `PortId`.
- Every visible real port publishes a stable anchor and hit geometry for the same frame revision.
- Dynamic rows must update node data and real ports through one atomic topology command.
- A decorative marker must never behave like a graph port.

The native node owns its content layout and reserves space for its labels, controls, and port
gutters. The runtime does not infer collision-free native content layout.

## Commands And Asynchronous Work

Build a `GraphCommand` from one owned, coherent facts view and preserve its opaque expected graph
version. Enqueue it once. Do not reconstruct that version from a numeric revision, merge with newer
state, or retry automatically.

When a native interaction emits a graph mutation after earlier native commands, capture an ordered
command-prefix barrier before releasing its optimistic presentation. Wait only for commands in that
captured prefix, then insert the interaction mutation ahead of later suffix work. Cancellation,
capture loss, owner replacement, retirement, and close must abandon the reservation explicitly.
Native pointer ownership may pass to a successor component; a late release from the old owner must
not clear the successor's capture or presentation.

Distinguish failures by evidence:

- A validation or enqueue failure before journal admission means the command was not dispatched.
- A conflict or rejection is a definite semantic terminal outcome.
- A transport, host, callback, or missing-receipt failure may have committed the command. Treat it as
  uncertain until a fresh authoritative observation resolves it.

Retirement, host close, task cancellation, and externalized native state must never replay a pending
command.

Application-owned asynchronous providers, such as connection search, receive owned authoritative
input before leaving the UI thread. Invoke and poll them on the framework's background executor,
make cancellation observable by dropping their future, and key publication by both semantic request
identity and product load generation. A cancelled, retired, or host-closed generation cannot update
the next search surface.

## Input And Focus

The native driver owns raw events and pointer capture. Feed normalized intent into one
`CanvasInteractionSession` and execute its effects in order. Do not maintain a parallel semantic
connection, reconnect, node-drag, pan, marquee, or search state machine.

Native controls must form interaction islands so text input, buttons, scrolling, menus, and overlays
do not arm graph gestures. Focus and IME stay framework-native. Cancellation, retirement, and host
close must release capture and settle pending semantic work exactly once.

## Geometry And Performance

Use published view geometry for markers, previews, committed routes, reconnect handles, picking, and
accessibility. Fallback geometry must be explicit and observable. A complete publication must not
allow preview and committed endpoints to disagree.

Keep work proportional to visible or affected facts:

- Do not scan the full graph on pointer movement.
- Do not allocate one retained native entity per edge.
- Keep paint and geometry caches bounded.
- Preserve incremental updates for local graph changes and multiple independent views.
- Ensure unchanged frames converge without recurring feedback or command work.

## Conformance

Enable the non-default `jellyflow-runtime` adapter-conformance feature in the adapter's test target.
Run the shared data-only scenarios through the adapter's real native event driver, then report the
normalized observations expected by the contract. The native driver must exercise actual framework
events, frames, capture, focus, geometry feedback, command receipts, retirement, and close cleanup;
calling the semantic session directly is not adapter conformance.

At minimum, cover new connections, both reconnect endpoints, node dragging, cancellation,
selection, conflicts, rejections, uncertain outcomes, command-prefix waiting, capture loss, owner
replacement, node retirement, and host close. Assertions must include optimistic presentation and
terminal receipt/cleanup observations, not only the final graph snapshot. Add lifecycle and
resource-bound tests specific to the framework without moving those framework details into the
shared contract.

See `jellyflow-open-gpui` and `jellyflow-egui` for production adapter implementations, and the
external consumer fixtures under `tools/fixtures/external-consumer` for direct dependency and
conformance examples.
