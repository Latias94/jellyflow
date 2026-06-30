# Jellyflow

Headless node/flow graph runtime for Rust.

[![CI](https://github.com/Latias94/jellyflow/actions/workflows/ci.yml/badge.svg)](https://github.com/Latias94/jellyflow/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/jellyflow.svg)](https://crates.io/crates/jellyflow)
[![Documentation](https://docs.rs/jellyflow/badge.svg)](https://docs.rs/jellyflow)
[![Crates.io Downloads](https://img.shields.io/crates/d/jellyflow.svg)](https://crates.io/crates/jellyflow)
[![Made with Rust](https://img.shields.io/badge/made%20with-Rust-orange.svg)](https://www.rust-lang.org)

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

Jellyflow owns the portable graph document model, undoable graph transactions, headless runtime
store, schema/profile pipeline, renderer-neutral interaction planners, layout engine boundary, and
adapter conformance fixtures for node graph editors.

The headless crates are extracted from Fret, but they do not depend on Fret UI, renderer, platform,
windowing, DOM, React, wgpu, GPUI, or egui crates. Product-specific rendering and input binding stay
in adapter crates such as `jellyflow-egui` and `jellyflow-open-gpui`.

## Choose Your Entry Point

| You want to... | Start with | Notes |
| --- | --- | --- |
| Build an editor or adapter with one Rust dependency | [`jellyflow`](https://crates.io/crates/jellyflow) | Facade crate that re-exports `core`, `layout`, and `runtime`, plus a small prelude. |
| Store and edit portable graph documents | [`jellyflow-core`](https://crates.io/crates/jellyflow-core) | IDs, graph data, ports, bindings, symbols, operations, transactions, fragments, and history. |
| Drive headless graph interaction behavior | [`jellyflow-runtime`](https://crates.io/crates/jellyflow-runtime) | Store dispatch, policy, schema/profile hooks, selection, delete, drag, resize, viewport, geometry, XyFlow-style projections, and conformance fixtures. |
| Add automatic layout without renderer dependencies | [`jellyflow-layout`](https://crates.io/crates/jellyflow-layout) | Built-in `dugong`, tidy tree, radial mind-map, and freeform mind-map engines, plus custom engine registration. |
| Embed an immediate-mode native graph editor | [`jellyflow-egui`](https://crates.io/crates/jellyflow-egui) | `eframe` adapter with a palette, toolbar, inspector, canvas rendering, pointer input, and demo app. |
| Build the retained GPUI adapter | [`jellyflow-open-gpui`](crates/jellyflow-open-gpui) | First-class Open GPUI adapter boundary for semantic surfaces, measurement publication, and capability reporting. |
| Prove a second adapter boundary | [`jellyflow-proof`](crates/jellyflow-proof) | Workspace proof crate that reuses the semantic surface and headless store without depending on egui. |
| Start a renderer integration | [`templates/headless-adapter`](templates/headless-adapter) | Copyable external adapter skeleton with conformance checks before renderer smoke tests. |

## What Jellyflow Provides

- Serializable graph documents with stable graph, node, port, edge, group, symbol, sticky-note, and
  binding IDs.
- Undoable `GraphTransaction` edits and graph fragments for copy/paste-style workflows.
- `NodeGraphStore` for dispatch, history, view state, subscriptions, middleware, and profile-aware
  transaction application.
- Renderer-neutral interaction planners for selection boxes, delete selection, connection targets,
  node dragging, node resizing, viewport pan/zoom, viewport animation, pan inertia, and auto-pan.
- Schema and profile APIs for custom node kinds, palette/view descriptors, aliases, default node
  data, port declarations, create-node dispatch, migration, and host validation.
- Store-level read APIs for deterministic render order, visible node/edge IDs, binding resolution,
  layout facts, measured handles, and connection target candidates.
- Layout engine registry with built-in layered DAG and mind-map families, plus host-provided custom
  engines.
- XYFlow-style editor primitives for custom node schemas, multiple handles, typed ports,
  selectable/draggable/connectable policy flags, node extent and parent expansion, view state,
  controlled-change projection, and renderer-owned node/edge presentation.
- Renderer-neutral product protocols for port view metadata, edge-owned data/view descriptors, and
  graph profile metadata for node fields, variable surfaces, validation hints, and connection-rule
  labels.
- Headless conformance fixtures that adapter crates can run before DOM, GPU, screenshot, or pixel
  tests.

## Install

```sh
# Main facade crate
cargo add jellyflow@0.3.0

# Narrow dependencies for lower-level consumers
cargo add jellyflow-core@0.3.0
cargo add jellyflow-layout@0.3.0
cargo add jellyflow-runtime@0.3.0
cargo add jellyflow-egui@0.3.0
```

From a local checkout:

```sh
cargo test -p jellyflow
cargo run -p jellyflow-runtime --example store_dispatch
cargo run -p jellyflow-runtime --example knowledge_canvas
cargo run -p jellyflow-runtime --example layout_engines
cargo run -p jellyflow-egui --example demo
cargo run -p jellyflow-egui --example workflow
cargo run -p jellyflow-egui --example automation_builder
cargo run -p jellyflow-egui --example mind_map
cargo run -p jellyflow-egui --example tree
cargo run -p jellyflow-egui --example org_chart
cargo run -p jellyflow-egui --example knowledge_board
cargo run -p jellyflow-egui --example erd
cargo run -p jellyflow-egui --example shader_graph
```

MSRV is `rust-version = 1.95`.

## Contents

- [Choose Your Entry Point](#choose-your-entry-point)
- [What Jellyflow Provides](#what-jellyflow-provides)
- [Install](#install)
- [Quickstart](#quickstart)
- [Custom Nodes And Adapters](#custom-nodes-and-adapters)
- [Layout Engines](#layout-engines)
- [Adapter Conformance](#adapter-conformance)
- [Protocol Boundaries](#protocol-boundaries)
- [Performance](#performance)
- [Developing](#developing)
- [Quality Gates](#quality-gates)
- [Limitations](#limitations)
- [Architecture Notes](#architecture-notes)
- [Workspace Crates](#workspace-crates)
- [Links](#links)

## Quickstart

For most Rust applications, start with `jellyflow::prelude::*`:

```rust
use jellyflow::prelude::*;

let graph = Graph::new(GraphId::new());
let store = NodeGraphStore::new(
    graph,
    NodeGraphViewState::default(),
    NodeGraphEditorConfig::default(),
);

assert_eq!(store.graph().nodes().len(), 0);
```

Use the lower-level module re-exports when you need a specific surface:

```rust
use jellyflow::{core, runtime};

let graph = core::Graph::new(core::GraphId::new());
let file = runtime::io::GraphFileV1::from_graph(graph);

assert_eq!(file.graph_version, 1);
```

Runnable examples:

```sh
cargo run -p jellyflow-core --example build_graph
cargo run -p jellyflow-runtime --example store_dispatch
cargo run -p jellyflow-runtime --example geometry_edge
cargo run -p jellyflow-runtime --example knowledge_canvas
cargo run -p jellyflow-runtime --example layout_engines
cargo run -p jellyflow-runtime --example dirty_scope_layout
cargo run -p jellyflow-egui --example demo
cargo run -p jellyflow-egui --example workflow
cargo run -p jellyflow-egui --example automation_builder
cargo run -p jellyflow-egui --example mind_map
cargo run -p jellyflow-egui --example tree
cargo run -p jellyflow-egui --example org_chart
cargo run -p jellyflow-egui --example knowledge_board
cargo run -p jellyflow-egui --example erd
cargo run -p jellyflow-egui --example shader_graph
```

## Custom Nodes And Adapters

Jellyflow treats custom nodes as headless schema data. Adapters register semantic node kinds through
`jellyflow_runtime::schema::NodeRegistry`, then build their own renderer registry from
`NodeRegistry::view_descriptors()`.

`NodeKindViewDescriptor.renderer_key` is adapter-owned data rather than a component reference, so
React, Svelte, native, egui, wgpu, and Fret-style adapters can map the same headless schema to
different renderer implementations while preserving graph semantics, ports, default data, default
size, category, and search metadata.

Ports can carry `PortViewDescriptor` metadata for side, order, group, anchor, lane, slot, label,
icon key, and handle visibility. Node schemas can also carry `NodeSurfaceSlotDescriptor` metadata
for semantic node-local slots such as headers, field rows, action rows, badges, metric badges,
validation/status banners, config groups, typed port rails, previews, and nested regions. Node
schemas can also declare adapter-owned chrome such as resize handles, toolbars, status strips, run
actions, validation banners, and inspector anchors. Edges can carry opaque domain data plus
`EdgeViewDescriptor` metadata for labels, renderer keys, markers, style tokens, route kind, and
hit-target width. These are normal graph fields, so they serialize, diff, undo/redo, and flow
through transaction footprints.
Adapter-only state such as hover, focused inputs, open menus, and transient connection previews
stays outside the graph.

For create-node palettes, adapters can call
`NodeGraphStore::apply_create_node_from_schema(registry, CreateNodeRequest::new(kind, pos))`. The
store uses the same dispatch, history, middleware, profile, and patch path as ordinary graph edits.

```rust
use jellyflow::prelude::*;
use jellyflow::runtime::schema::{
    NodeRegistry, NodeSchema, NodeSurfaceSlotDescriptor, PortDecl,
};

let mut registry = NodeRegistry::new();
registry.register(
    NodeSchema::builder("task.card", "Task Card")
        .category(["Workflow", "Tasks"])
        .keywords(["todo", "kanban"])
        .renderer_key("task-card")
        .surface_slot(NodeSurfaceSlotDescriptor::header("header.main"))
        .surface_slot(NodeSurfaceSlotDescriptor::field_row("field.source"))
        .default_size(CanvasSize {
            width: 180.0,
            height: 104.0,
        })
        .port(PortDecl::data_input("source").with_label("Source").on_left())
        .port(PortDecl::data_output("result").with_label("Result").on_right())
        .build(),
);

let descriptors = registry.view_descriptors();
assert_eq!(descriptors[0].renderer_key, "task-card");
```

Custom node checklist:

- Put reusable semantics in node kits or schemas: ports, slot kinds, `slot` data paths, `anchor`
  placement keys, default data, layout hints, and adapter chrome descriptors.
- Keep toolkit widgets adapter-local. egui, GPUI, Dioxus, DOM, and other hosts map the same
  `renderer_key` and slot descriptors to their own components.
- Treat `jellyflow-open-gpui` as the mature retained-adapter target for this stage. It owns GPUI
  adapter contracts and measurement conversion, while the `repo-ref/open-gpui` example stays a
  consumer and visual fixture.
- Report measurements after rendering rich internals. Slot, anchor, handle, node-size, density, and
  invalidation facts are the bridge that keeps handles, edges, hit tests, and resize previews aligned
  with real UI.
- Trigger node-internal invalidation when node data, component state, zoom, or size changes enough
  to move internal slots or handles.
- Keep backend execution outside Jellyflow. LLM calls, shader compilation, database queries,
  schedulers, collaboration, and persistence services belong in the host product.

See [`docs/examples/README.md`](docs/examples/README.md) for Dify-style workflow, shader/blueprint,
ERD/table, and knowledge-canvas gallery coverage.

Use `jellyflow_runtime::profile::GraphProfileMetadata` when a product needs reusable field schemas,
variable surfaces, validation hints, or connection-rule labels without creating a domain crate. A
future `jellyflow-workflow` crate is intentionally deferred until the workflow and Dify-like
fixtures prove those profile rules repeat cleanly outside local examples.

For rendering loops, subscribe to small store projections and then query the read model your
renderer needs. Jellyflow returns stable IDs and render order; adapters keep component state,
memoization, and renderer lookup outside the graph document.

```rust
let _token = store.subscribe_selector(
    |snapshot| (snapshot.graph_revision, snapshot.layout_facts_revision),
    |_| {
        // Re-query visible IDs from the host event loop or adapter state.
    },
);

let rendering = store.rendering_query(CanvasSize {
    width: 1280.0,
    height: 720.0,
});
```

## Layout Engines

`jellyflow-layout` keeps automatic layout outside the core document model. Layout engines receive a
graph projection and return `LayoutResult` or normal `GraphTransaction` values that hosts can apply,
animate, or discard.

Built-in engines:

- `dugong`: layered DAG layout.
- `tidy_tree`: tree layout that centers parents over children for hierarchy and outline views.
- `mind_map_radial`: radial mind-map layout.
- `mind_map_freeform`: overlap-resolving freeform mind-map layout that respects pinned nodes.

Typical user-facing graph shapes map to the same headless model with different schemas, renderer
keys, and layout presets:

- Workflow and automation editors use directed task, decision, and output nodes with layered DAG
  layout.
- MindNode-style mind maps use topic/idea nodes with radial or freeform mind-map layout.
- Outline and organization views use section nodes with tidy-tree layout.
- MarginNote-style knowledge boards use source, claim, question, and action cards with freeform
  layout, explicit positions, and normal typed edges between cards.

Custom engines implement `LayoutEngine` and register with `LayoutEngineRegistry`. Runtime callers
can build a `LayoutContext` from store measurements and binding pins through
`NodeGraphStore::layout_context()` or `NodeGraphStore::layout_context_with_binding_pins()`.
For common cases, `LayoutPresetBuilder` builds ordinary `LayoutEngineRequest` values for workflow,
tree, radial mind-map, and freeform mind-map layouts, so hosts can start from a preset and still
override engine IDs, direction, spacing, scope, and measured sizes.

After a store dispatch, use `DispatchOutcome::footprint()` to derive a conservative layout dirty
scope. This keeps high-frequency re-layouts focused on nodes touched directly by the committed
transaction, or by the ports, edges, and bindings referenced by that transaction.

```rust
let request = LayoutRequest::all()
    .with_dirty_scope_from_footprint(store.graph(), outcome.footprint());
```

```sh
cargo run -p jellyflow-runtime --example layout_engines
cargo run -p jellyflow-runtime --example dirty_scope_layout
```

## Adapter Conformance

Jellyflow keeps XyFlow-feel checks at the headless runtime boundary before renderer smoke tests.
Conformance fixtures drive a real `NodeGraphStore` and compare normalized traces for graph commits,
view changes, gesture lifecycle events, and XyFlow-style callback projections.

Run fixture suites with:

```sh
cargo run -p jellyflow-runtime --example conformance_harness -- check <fixture-dir>
cargo run -p jellyflow-runtime --example conformance_harness -- approve <fixture-dir>
```

Start adapter work from the copyable template:

```sh
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

Renderer-specific pointer capture, DOM measurement, accessibility text, GPU resources, screenshots,
and pixel tests belong in adapter crates. Jellyflow owns the deterministic headless behavior that
those adapters consume.

## Protocol Boundaries

The canonical model is always the Jellyflow graph plus `DispatchOutcome` and `NodeGraphPatch`.
`runtime::xyflow` is a compatibility dialect for hosts that want XyFlow-style node and edge change
arrays. It is useful for adapter integration, but it is not the full persistence format.

| Surface | XyFlow-style compatibility | Full-fidelity Jellyflow path |
| --- | --- | --- |
| Node and edge add/remove/update changes | Supported through `NodeGraphChanges` and apply helpers. | `GraphTransaction`, `DispatchOutcome`, and graph snapshots. |
| Node data, dimensions, position, selection, and interaction policy | Supported where there is an XyFlow-shaped change. | Graph fields, view state, policy, and transaction footprints. |
| Edge kind, endpoints, data, and `EdgeViewDescriptor` | Supported by Jellyflow edge change variants in the compatibility dialect. | Edge-owned graph fields with serde, diff, undo/redo, and footprints. |
| Port view metadata, profile metadata, bindings, groups, sticky notes, and layout facts | Not represented as ordinary XyFlow node/edge arrays. | Schema descriptors, `GraphProfileMetadata`, binding/layout queries, and patches. |
| Adapter-local hover, focus, open menus, and connection previews | Not persisted or projected. | Adapter state only. |

Unsupported compatibility fields must not clear full-fidelity graph data. If an integration needs
lossless state, use the store patch and graph APIs instead of only `NodeGraphChanges`.

New domain protocols are promoted only after they are reused by at least two product fixtures, or by
one product fixture plus external consumer smoke coverage, and have public-surface tests plus a
renderer-independent conformance path. Until then, workflow, ERD, mind-map, org-chart, and
knowledge-board semantics stay as examples and profile/schema fixtures rather than new domain
crates.

## Performance

The workspace includes Criterion benchmarks for:

- `rendering_query`: large-graph visible ordering and culling reads.
- `schema_create_node`: schema descriptor enumeration, node instantiation, and store-level
  schema-driven node creation.
- `layout_engines`: `tidy_tree` and `dugong` layout throughput for tree and layered DAG fixtures.
- `layout_pipeline`: runtime layout context, planning, transaction conversion, and apply pipeline
  costs.

Run local measurements with:

```sh
cargo bench -p jellyflow-layout --bench layout_engines
cargo bench -p jellyflow-layout --features benchmark-internals --bench layout_engines
cargo bench -p jellyflow-runtime --bench rendering_query
cargo bench -p jellyflow-runtime --bench schema_create_node
cargo bench -p jellyflow-runtime --bench layout_pipeline
```

For a dugong stage breakdown on the same layered DAG fixture sizes, run:

```sh
DUGONG_DAGREISH_TIMING=1 DUGONG_ORDER_TIMING=1 cargo run -p jellyflow-layout --example dugong_timing
```

CI runs the same benchmarks in Criterion `--test` mode to catch broken benchmark fixtures without
treating hosted runner timing as a regression signal. See
[`docs/reviews/runtime-performance-baseline-2026-06-12.md`](docs/reviews/runtime-performance-baseline-2026-06-12.md)
for the current baseline and optimization rules.

## Developing

For local Rust changes, start with:

```sh
cargo fmt --all --check
cargo check --workspace --locked
cargo nextest run --workspace
```

Use these before publishing or changing public adapter behavior:

```sh
cargo clippy --workspace --all-targets -- -D warnings
python3 tools/check_no_fret_dependencies.py
python3 tools/check_external_consumer_smoke.py
cargo bench -p jellyflow-layout --features benchmark-internals --bench layout_engines -- --test
cargo bench -p jellyflow-runtime --bench rendering_query -- --test
cargo bench -p jellyflow-runtime --bench schema_create_node -- --test
cargo bench -p jellyflow-runtime --bench layout_pipeline -- --test
git diff --check
```

Crates.io release notes, version checks, and publish order live in
[`docs/releasing/CRATES_IO.md`](docs/releasing/CRATES_IO.md).

Release CI follows the same split as the source Rust workspace this project was extracted from:
`release-preflight` is a manual publish-readiness check, `release-crates` publishes the five crates
in dependency order from a `v*` tag or manual dispatch, and `release` creates or updates the GitHub
Release notes for the same tag.

## Quality Gates

Jellyflow's CI is built around three checks:

- Workspace correctness: format, locked check, nextest, clippy, and benchmark smoke.
- Boundary hygiene: Cargo tree checks reject accidental `fret` or `fret-*` dependencies.
- External consumption: temporary non-workspace consumers compile against the facade crate, the
  lower-level crates, and the headless adapter template.

Release preflight also records package file lists for all publishable crates using Cargo's package
view, so release review can catch accidental `repo-ref`, historical workstream, or renderer/platform
assets before publishing.

## Limitations

- The core `jellyflow`, `jellyflow-core`, `jellyflow-layout`, and `jellyflow-runtime` crates are
  headless. Use adapter crates such as `jellyflow-egui` for rendered editor surfaces.
- It does not own raw pointer capture, keyboard focus policy, DOM/React provider state, browser
  measurement APIs, GPU resources, screenshot tests, or pixel comparisons.
- `0.x` is still an early public line. Public APIs are intentionally small but may still change as
  adapter crates harden their integration patterns.
- First-time crates.io dry-runs for dependent crates can fail until earlier Jellyflow crates are
  visible on crates.io. Publish in dependency order.

## Architecture Notes

- `jellyflow-core` owns portable graph data, IDs, validation, transactions, fragments, and history.
- `jellyflow-layout` owns optional headless layout engines and layout discovery metadata.
- `jellyflow-runtime` owns store behavior, interaction planning, schema/profile pipeline,
  renderer-neutral geometry, rendering queries, binding/layout-facts reads, XyFlow compatibility
  projections, and conformance fixtures.
- `jellyflow` is a thin facade. It re-exports the lower-level crates and common entry points without
  adding another behavior layer.
- `jellyflow-egui` owns the immediate-mode egui adapter surface and depends on the facade instead of
  the lower-level crates directly.
- `jellyflow-open-gpui` owns the retained Open GPUI adapter boundary for this stage. It keeps
  toolkit widget types out of runtime/core and reports projection fallback honestly until the GPUI
  host publishes real layout-pass bounds.
- `fret-node` remains the Fret adapter and compatibility facade in the Fret repository.

This repository was created by a history-preserving path-filtered extraction from Fret. The source
Fret commit is recorded in [`JELLYFLOW_SOURCE_COMMIT.txt`](JELLYFLOW_SOURCE_COMMIT.txt), and the
filter command is recorded in
[`docs/extraction/EXTRACTION_RECORD_2026-05-30.md`](docs/extraction/EXTRACTION_RECORD_2026-05-30.md).

## Workspace Crates

| Crate | Role |
| --- | --- |
| [`jellyflow`](https://crates.io/crates/jellyflow) | Public Rust facade over core, layout, and runtime. |
| [`jellyflow-core`](https://crates.io/crates/jellyflow-core) | Portable graph document model, IDs, validation, transactions, fragments, and history. |
| [`jellyflow-layout`](https://crates.io/crates/jellyflow-layout) | Optional headless layout engines and custom layout registry. |
| [`jellyflow-runtime`](https://crates.io/crates/jellyflow-runtime) | Headless store, rules, schema/profile pipeline, interaction planners, geometry, rendering reads, XyFlow projections, and conformance fixtures. |
| [`jellyflow-egui`](https://crates.io/crates/jellyflow-egui) | Immediate-mode egui adapter with a demo app, canvas, palette, toolbar, and inspector. |
| [`jellyflow-open-gpui`](crates/jellyflow-open-gpui) | Retained Open GPUI adapter boundary for node surface projection, measurement publication, and capability facts. |
| [`jellyflow-proof`](crates/jellyflow-proof) | Workspace proof crate for a second adapter boundary and semantic-surface reuse. |

## Links

- Changelog: [`CHANGELOG.md`](CHANGELOG.md)
- Publishing guide: [`docs/releasing/CRATES_IO.md`](docs/releasing/CRATES_IO.md)
- Performance baseline: [`docs/reviews/runtime-performance-baseline-2026-06-12.md`](docs/reviews/runtime-performance-baseline-2026-06-12.md)
- Adapter boundary ADR: [`docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`](docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md)
- Layout extension ADR: [`docs/adr/0005-layout-engine-extension-boundary.md`](docs/adr/0005-layout-engine-extension-boundary.md)
- License: dual MIT or Apache-2.0; see [`LICENSE-MIT`](LICENSE-MIT) and
  [`LICENSE-APACHE`](LICENSE-APACHE)
