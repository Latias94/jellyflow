# Retained Native Open GPUI Jellyflow Example

This package is Jellyflow's retained native Open GPUI workflow and shader-graph product example.

It intentionally lives outside the Jellyflow root workspace so the default
Jellyflow checks do not pull the Open GPUI native stack into the release
workspace. The example depends on the published `open-gpui-*` 0.2 crates while
using local Jellyflow path dependencies for the unreleased Jellyflow workspace
crates.

The executable starts with `GraphRuntime`, attaches one `ViewSession`, and mounts a
`NativeGraphSurface`. The surface owns the sealed `OpenGpuiSceneHost` alongside the
application-owned blackboard and dropped-wire search surfaces. Shader nodes are ordinary
retained Open GPUI entities. They compose native controls directly and publish real port
geometry through `OpenGpuiPortHandle`; no cross-framework node UI DSL or descriptor projection
is involved.

The window also mounts an application-owned native blackboard beside the graph. It reads graph
symbols directly from `GraphRuntime` snapshots and submits version-checked `SymbolEdit` commands;
the adapter does not define a portable blackboard widget or descriptor.

Workflow and shader fields share application-owned `RetainedFieldStore<T>` handles. Each native
field mounts a `RetainedField<T>` with an exact node incarnation and automatically binds the store
to its view. Dirty values can survive protected-cache overflow without moving product state into the
headless runtime, restoring IME state, or retrying an interrupted command.

Dropping a shader wire on empty canvas opens another application-owned Open GPUI surface. It uses
the host's inert create-from-wire request, semantic candidate query, and atomic commit APIs, then
owns filtering, empty/error states, stale-result refresh, retry, cancellation, and focus
restoration with ordinary native controls. This is the intended authoring boundary for Dify- and
Shader Graph-class node tooling: the runtime owns graph facts and commands, while the application
owns the node-internal UI and interaction presentation.

Ordinary wire and reconnect commits use a separate structured result channel. The surface consumes
each result once, renders stable rejection codes and messages in native UI, and clears stale
feedback after the next successful commit; create-from-wire search keeps its own result lifecycle.

Run it from the Jellyflow root:

```sh
cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked

# Dify-style workflow nodes
cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked -- --workflow
```

Run the two native-window readiness checks on a machine with a window server:

```sh
cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --bin open-gpui-real-window-smoke \
  --locked --features real-window-smoke
cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --bin open-gpui-real-window-smoke \
  --locked --features real-window-smoke -- --workflow
```

CI builds the smoke binary once and runs the same two commands directly. Each process opens the
production Shader or Workflow window, waits for a positive native surface, a live view, mounted
nodes, and published node/port/route geometry, then exits. It reports failures on stderr and uses
the process exit status as the complete smoke protocol.

Interaction semantics do not belong in this process-level smoke. Drag, reconnect, connection
search, focus, content-size convergence, semantic zoom, and selection chrome are covered by the
deterministic runtime and Open GPUI tests, where failures are reproducible and do not depend on
window-manager bounds, compositor pixels, or synthetic input timing. See the
[node UI authoring regression contract](../../docs/testing/node-ui-authoring-regression.md).

Run the focused product gates:

```sh
cargo nextest run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  --workspace --all-features --locked --no-fail-fast
cargo clippy --manifest-path examples/canvas-jellyflow/Cargo.toml \
  --workspace --all-targets --all-features --locked -- -D warnings
```

The hard adapter contracts remain in `jellyflow-open-gpui`. This example owns
only the product graph, retained native node entities, and application entrypoint.
