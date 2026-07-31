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

Run both real-window smoke modes on a machine with a native window server:

```sh
cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked --features real-window-smoke -- --smoke
cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked --features real-window-smoke -- --workflow --smoke
```

Smoke mode drives the production window through the bounded Shader and Workflow interaction matrix.
A pass requires retained lifecycle invariants, complete current-graph geometry, non-blank native
paint, fixed-zoom node and connection auto-pan, focus traversal, edge deletion and reconnect,
create-from-wire search, fitted selection chrome, and the directed semantic-LOD continuity probe.
The process emits one `JELLYFLOW_REAL_WINDOW_SMOKE_RECEIPT` JSON receipt and exits automatically;
any failed invariant, early close, or timeout exits non-zero. The matrix runner validates that
receipt through the Rust `smoke_receipt_contract` binary before accepting it. See the
[node UI authoring regression contract](../../docs/testing/node-ui-authoring-regression.md) for the
authoritative scenarios, window and zoom matrix, screenshot checkpoints, and required commands.
The non-default `real-window-smoke` feature only enables framebuffer readback for this gate; normal
product builds do not enable test support.

The production frame-clock witness is a smaller, non-visual gate for native node-drag auto-pan. It
builds without `real-window-smoke`, holds a real node drag in the viewport edge band, requires at
least three monotonically moving viewport samples entered through `Window::on_next_frame`, and exits
automatically:

```sh
cargo build --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked --no-default-features \
  --bin open-gpui-native-frame-clock-witness
python3 tools/check_open_gpui_native_frame_clock.py --require-native
```

Run the focused tests:

```sh
cargo nextest run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked --no-fail-fast
```

The hard adapter contracts remain in `jellyflow-open-gpui`. This example owns
only the product graph, retained native node entities, and application entrypoint.
