# jellyflow-runtime

`jellyflow-runtime` is the renderer-neutral graph and view protocol for Jellyflow. It sits between
the portable document model in `jellyflow-core` and framework-native adapters.

```text
GraphRuntime -> ViewSession -> native adapter
```

## Responsibilities

- `GraphRuntime` owns the canonical graph, document revision, profiles, connection policy,
  revision-checked commands, subscriptions, and undo/redo history.
- `ViewSession` owns view-local viewport state, selection, culling, routing, geometry, frame
  tokens, and feedback lifecycle. Multiple sessions can observe one runtime without sharing view
  state.
- `AdapterFrame` and `FeedbackCollector` form the native adapter boundary. An adapter requests an
  owned frame, renders its own components, records complete node and real-port geometry, appends
  graph intents in host order, and submits the feedback once.
- `NodeCatalog` and `NodeKindSpec` contain creation metadata, defaults, stable port declarations,
  connection typing, renderer keys, and small presentation hints. They do not describe controls,
  forms, menus, inspectors, repeatables, or widget layout.
- `io` persists canonical graph files and sanitized editor-state seeds. Framework focus, IME,
  drafts, retained entities, overlays, geometry caches, and themes are not document fields.

## Minimal setup

For a graph created programmatically in the current process:

```rust
use jellyflow_runtime::{GraphRuntime, ViewSessionOptions};
use jellyflow_runtime::schema::NodeCatalog;
use jellyflow_core::{Graph, GraphId};

let runtime = GraphRuntime::builder(NodeCatalog::new()).build(Graph::new(GraphId::new()))?;
let view = runtime.attach_view(ViewSessionOptions::default())?;
assert_eq!(view.graph_snapshot()?.revision(), 0);
# Ok::<(), Box<dyn std::error::Error>>(())
```

For a persisted document, register known kinds and migrators first, then publish only the canonical
result:

```rust
use jellyflow_runtime::GraphRuntime;
use jellyflow_runtime::io::GraphFile;
use jellyflow_runtime::schema::NodeCatalog;

let catalog = NodeCatalog::new(); // Register application kinds and migrators here.
let document = GraphFile::load_canonical_json("workflow.jellyflow.json", &catalog)?;
let runtime = GraphRuntime::from_canonical(document)?;

let replacement =
    GraphFile::load_canonical_json("next-workflow.jellyflow.json", runtime.catalog())?;
runtime.replace_document(replacement)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

The persisted-file API does not expose a raw graph load or `Deserialize` path. This keeps exact
file-version dispatch, storage validation, and atomic known-kind migration ahead of runtime
publication. `GraphRuntime::replace_document` accepts only a `CanonicalGraph` produced by the same
catalog provenance. On success it advances the graph revision and document epoch, clears history,
retires every prior node incarnation, and fans out one document reset. Finalize registration before
runtime publication and use `runtime.catalog()` for later loads; an independently rebuilt or
subsequently modified catalog is rejected without side effects.

`GraphFile::save_json` writes the current V2 envelope and applies the default `PersistenceLimits`
before atomically replacing the target. Released Jellyflow 0.3 V1 graph documents are intentionally
incompatible and return `GraphFileError::UnsupportedEnvelopeVersion` before payload decoding; the
runtime does not expose a V1 facade or discard removed focusability fields. Applications with
custom persistence budgets should pass the same limits to `save_json_with_limits` and
`load_canonical_json_with_limits`. `ViewSessionSeedFile` decode and load operations accept only the
current versioned seed format and return a sanitized `ViewSessionSeed`.

Graph writes use `GraphCommand` and an expected `GraphRevision`. A stale command is rejected with
the current revision and affected footprint; the runtime never silently merges application intent.
Topology edits replace the complete ordered real-port set atomically and apply their declared
incident-edge policy in the same history boundary.

## Connection and layout facts

Adapters should query `ViewSession::port_connection_facts` and
`ViewSession::create_from_wire_candidates` for candidate filtering, then submit `GraphCommand`
connection or reconnect edits. Direction, type compatibility, capacity, replacement, cycle policy,
and rejection reasons remain runtime-owned.

Layout feedback must include the node root and every real port admitted by the frame. The runtime
validates each node independently and publishes accepted bounds and port hit geometry under one
view layout revision. Missing, foreign, duplicate, non-finite, stale, or replayed feedback has no
partial side effect.

## Boundary guarantees

The crate has no dependency on Open GPUI, egui, windowing, focus, IME, GPU, or platform APIs. A
native adapter owns its framework component tree, input arbitration, accessibility, measurement
implementation, and application services. Jellyflow only supplies graph semantics, stable IDs,
revisioned commands, and geometry facts.

Run the focused runtime tests with:

```sh
cargo nextest run -p jellyflow-runtime --cargo-quiet
```
