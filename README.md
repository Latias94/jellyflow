# Jellyflow

Headless node-graph runtime for native Rust UI frameworks.

[![CI](https://github.com/Latias94/jellyflow/actions/workflows/ci.yml/badge.svg)](https://github.com/Latias94/jellyflow/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/jellyflow-runtime.svg)](https://crates.io/crates/jellyflow-runtime)
[![Documentation](https://docs.rs/jellyflow-runtime/badge.svg)](https://docs.rs/jellyflow-runtime)

Jellyflow separates an authoritative graph runtime from framework-owned component trees:

```text
GraphRuntime -> ViewSession -> native adapter
```

The runtime owns graph documents, version-checked commands, connection policy, history, frame facts,
and view-isolated geometry. Adapters render ordinary native components, report complete node and
real-port bounds, and keep focus, IME, scrolling, popovers, themes, and local component state in
their UI framework.

Open GPUI is the first product-tier retained adapter. egui is a narrow immediate-mode protocol
witness. Jellyflow does not define a portable widget tree or cross-framework node UI DSL.

## Workspace

| Crate | Role |
| --- | --- |
| `jellyflow-core` | Portable graph facts, stable IDs, validation, type descriptors, and mutation footprints. |
| `jellyflow-layout` | Optional headless layout engines and custom layout registration. |
| `jellyflow-runtime` | `GraphRuntime`, `ViewSession`, version-checked commands, private mutation/history kernel, semantic catalog, persistence, frames, and geometry. |
| `jellyflow-open-gpui` | Retained Open GPUI registry, scene host, binding, measurement, input, activity, and draft lifecycle. |
| `jellyflow-egui` | Immediate-mode frame/feedback adapter and conformance witness. |

## Quickstart

```rust
use jellyflow_core::{Graph, GraphId};
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::schema::NodeCatalog;
use jellyflow_runtime::view::ViewSessionOptions;

let runtime = GraphRuntime::builder(NodeCatalog::new()).build(Graph::new(GraphId::new()))?;
let view = runtime.attach_view(ViewSessionOptions::default())?;
let snapshot = view.graph_snapshot()?;

assert_eq!(snapshot.revision(), 0);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Graph writes are checked against an opaque graph version. Execution always returns an
identity-keyed completion envelope; converting that envelope into its runtime result preserves the
current owned snapshot and affected footprint for stale commands. Jellyflow never silently retries
or merges application intent.

```rust
# use jellyflow_core::{Graph, GraphId};
# use jellyflow_runtime::commands::{GraphCommand, GraphCommandOutcome, TopologyEdit};
# use jellyflow_runtime::graph::GraphRuntime;
# use jellyflow_runtime::schema::NodeCatalog;
# let runtime = GraphRuntime::builder(NodeCatalog::new()).build(Graph::new(GraphId::new()))?;
let outcome = runtime
    .execute(GraphCommand::topology(
        runtime.snapshot()?.version(),
        TopologyEdit::new(),
    ))
    .into_result()?;

assert!(matches!(outcome, GraphCommandOutcome::Noop(_)));
# Ok::<(), Box<dyn std::error::Error>>(())
```

After an adapter resolves drag, resize, or layout interaction policy, commit the resulting node
geometry atomically with one batch command:

```rust
# use jellyflow_core::{CanvasPoint, CanvasSize, Graph, GraphId, NodeId};
# use jellyflow_runtime::commands::{GraphCommand, NodeGeometryEdit};
# use jellyflow_runtime::graph::GraphRuntime;
# use jellyflow_runtime::schema::NodeCatalog;
# let runtime = GraphRuntime::builder(NodeCatalog::new()).build(Graph::new(GraphId::new()))?;
# let version = runtime.snapshot()?.version();
# let first = NodeId::new();
# let second = NodeId::new();
let command = GraphCommand::set_node_geometry(
    version,
    [
        NodeGeometryEdit::new(first)
            .with_position(CanvasPoint { x: 40.0, y: 80.0 }),
        NodeGeometryEdit::new(second)
            .with_position(CanvasPoint { x: 320.0, y: 180.0 })
            .with_size(Some(CanvasSize { width: 240.0, height: 160.0 })),
    ],
)
.with_label("move selection");
# let _ = command;
```

## Native Nodes

Applications register graph semantics in `NodeCatalog`: canonical kind/version identity, aliases,
default graph data, and stable real-port identity, direction, kind, capacity, type, and data. Node
creation supplies explicit initial bounds. Product discovery metadata, initial geometry, port
labels/layout hints, and native renderer mappings deliberately live outside the headless runtime.
The flagship example validates those three owners before exposing a node as creatable.

Open GPUI authors register retained `Entity<V: Render>` factories with
`OpenGpuiNodeRegistryBuilder`. Their node internals are normal Open GPUI controls. The shared
adapter adds only graph-semantic capabilities: `OpenGpuiPortHandle` publishes real ports,
`OpenGpuiNodeDragHandle` marks an explicit header or grip, and
`OpenGpuiOverlayInteractionBoundary` owns popup hit testing outside node bounds. Ordinary controls
and scroll areas need no Jellyflow wrapper. A native control builds an explicit command from an
owned binding read and submits it through managed authoring; runtime completion and authoritative
snapshots are reconciled without exposing raw tickets to product renderers. Commands never mutate
the runtime synchronously during render.

Renderer registration also selects explicit scale and size authority. The default semantic LOD
keeps arbitrary controls at native metrics, switches to canonical overview with hysteresis, and
uses a graph-inert pinned-detail surface for active focus, IME, or overlays. Continuous scaling is
an explicit component-managed responsibility. Document size remains the default; Dify-style
growing nodes opt into `ContentHeight`, wrap one 1:1 root with `binding.measure_content`, and
converge through version-checked geometry commands rather than render-time mutation.

See:

- `examples/canvas-jellyflow`: retained workflow and typed shader nodes.
- `crates/jellyflow-egui`: immediate-mode adapter witness without product-level widget parity.
- `docs/guides/native-adapter-authoring.md`: direct-dependency adapter ownership and verification guide.

Dynamic handles must be real graph `Port` records with stable `PortId`s. Adapter-local visual
handles never enter routing, hit testing, or graph transactions.

## Run The Product Example

The Open GPUI product workspace contains both flagship native graphs. On macOS, run Shader Graph
with:

```sh
cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked
```

Run the Dify-style workflow with:

```sh
cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked -- --workflow
```

## View Protocol

Each native canvas attaches an independent `ViewSession` to a shared `GraphRuntime`:

1. Request an owned `AdapterFrame` with `FrameRequest`.
2. Render ordinary framework-native components.
3. Collect complete root and real-port geometry after native layout.
4. Append graph commands, selection/viewport updates, and layouts to the frame's
   `FeedbackCollector` in host order.
5. Finish once and submit through the originating view.

Feedback tokens are one-shot and replay protected. View closure is terminal. Renderer replacement,
node lifetime replacement, document replacement, and environment changes invalidate stale work
without reviving retired hit targets.

Connection UI should use `ViewSession::evaluate_connection`, `port_connection_facts`, and
`create_from_wire_candidates`. The runtime remains authoritative for direction, type, capacity,
replacement, reconnect, cycle policy, and structured rejection diagnostics.

## Layout

`jellyflow-layout` provides layered DAG, tidy-tree, radial mind-map, and freeform mind-map engines.
Engines return `LayoutResult`; applications turn its node positions into a version-checked
`GraphCommand::set_node_geometry` commit. Framework measurements may be supplied through
`LayoutContext` without entering the graph document. The resulting command uses the opaque graph
version from the snapshot against which the layout was planned.

## Persistence

Graph files persist durable graph facts. Editor-state seeds persist only sanitized view state.
Live native focus, IME composition, drafts, geometry, retained entities, overlays, tasks, caches,
and themes are never graph-document fields.

Register the application's known kinds and migrators before loading a persisted graph. The public
file loader returns a `CanonicalGraph`, which is the persisted-document input to the runtime:

```rust
use jellyflow_runtime::io::GraphFile;
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::schema::NodeCatalog;

let catalog = NodeCatalog::new(); // Register application kinds and migrators here.
let document = GraphFile::load_canonical_json("workflow.jellyflow.json", &catalog)?;
let runtime = GraphRuntime::from_canonical(document)?;

let replacement =
    GraphFile::load_canonical_json("next-workflow.jellyflow.json", runtime.catalog())?;
runtime.replace_document(replacement)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`GraphRuntime::builder(catalog).build(graph)` is the entry point for graphs created
programmatically. It runs whole-graph canonicalization before publication and retains that exact
catalog snapshot for every attached view and adapter. Whole-document replacement accepts only a
`CanonicalGraph` produced from the same catalog provenance, then atomically advances the revision
and document epoch, clears history, retires prior node lifetimes, and resets every attached
view. Finalize catalog registration before publishing the runtime and use `runtime.catalog()` for
later loads; an independently rebuilt or subsequently modified catalog is intentionally rejected.
The supported persisted-file API likewise does not return a raw `Graph` without catalog migration.

`GraphFile` writes and accepts the current V2 envelope. Released Jellyflow 0.3 V1 documents,
including node or edge `focusable` fields, are intentionally incompatible and return a structured
`UnsupportedEnvelopeVersion` error before payload decoding; removed fields are never silently
dropped during decoding. Within V2, registered node-kind migrations remain atomic and
unknown node kinds remain round-trippable.

## Development

```sh
cargo fmt --all --check
cargo nextest run --workspace --locked --cargo-quiet
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 tools/check_no_fret_dependencies.py
python3 -m unittest discover -s tools/tests -p "test_*.py"
git diff --check
```

Native adapter gates:

```sh
cargo fmt --manifest-path examples/canvas-jellyflow/Cargo.toml --all -- --check
cargo nextest run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  --workspace --all-features --locked --no-fail-fast
cargo test --manifest-path examples/canvas-jellyflow/Cargo.toml \
  --workspace --all-features --doc --locked
cargo clippy --manifest-path examples/canvas-jellyflow/Cargo.toml \
  --workspace --all-targets --all-features --locked -- -D warnings
```

MSRV is Rust 1.95. The core, layout, and runtime crates must remain free of UI framework,
windowing, focus, IME, GPU, and platform dependencies.

## Links

- [Publishing guide](docs/releasing/CRATES_IO.md)
- [Native adapter authoring guide](docs/guides/native-adapter-authoring.md)
- [Native node regression gates](docs/testing/node-ui-authoring-regression.md)
- Dual MIT or Apache-2.0 license: [MIT](LICENSE-MIT), [Apache-2.0](LICENSE-APACHE)
