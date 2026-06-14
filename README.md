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

It is extracted from Fret, but it does not depend on Fret UI, renderer, platform, windowing, DOM,
React, wgpu, or egui crates. Product-specific rendering and input binding stay in adapter crates.

## Choose Your Entry Point

| You want to... | Start with | Notes |
| --- | --- | --- |
| Build an editor or adapter with one Rust dependency | [`jellyflow`](https://crates.io/crates/jellyflow) | Facade crate that re-exports `core`, `layout`, and `runtime`, plus a small prelude. |
| Store and edit portable graph documents | [`jellyflow-core`](https://crates.io/crates/jellyflow-core) | IDs, graph data, ports, bindings, symbols, operations, transactions, fragments, and history. |
| Drive headless graph interaction behavior | [`jellyflow-runtime`](https://crates.io/crates/jellyflow-runtime) | Store dispatch, policy, schema/profile hooks, selection, delete, drag, resize, viewport, geometry, XyFlow-style projections, and conformance fixtures. |
| Add automatic layout without renderer dependencies | [`jellyflow-layout`](https://crates.io/crates/jellyflow-layout) | Built-in `dugong`, tidy tree, radial mind-map, and freeform mind-map engines, plus custom engine registration. |
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
- Headless conformance fixtures that adapter crates can run before DOM, GPU, screenshot, or pixel
  tests.

## Install

```sh
# Main facade crate
cargo add jellyflow@0.2.0

# Narrow dependencies for lower-level consumers
cargo add jellyflow-core@0.2.0
cargo add jellyflow-layout@0.2.0
cargo add jellyflow-runtime@0.2.0
```

From a local checkout:

```sh
cargo test -p jellyflow
cargo run -p jellyflow-runtime --example store_dispatch
cargo run -p jellyflow-runtime --example knowledge_canvas
cargo run -p jellyflow-runtime --example layout_engines
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

assert_eq!(store.graph().nodes.len(), 0);
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
```

## Custom Nodes And Adapters

Jellyflow treats custom nodes as headless schema data. Adapters register semantic node kinds through
`jellyflow_runtime::schema::NodeRegistry`, then build their own renderer registry from
`NodeRegistry::view_descriptors()`.

`NodeKindViewDescriptor.renderer_key` is adapter-owned data rather than a component reference, so
React, Svelte, native, egui, wgpu, and Fret-style adapters can map the same headless schema to
different renderer implementations while preserving graph semantics, ports, default data, default
size, category, and search metadata.

For create-node palettes, adapters can call
`NodeGraphStore::apply_create_node_from_schema(registry, CreateNodeRequest::new(kind, pos))`. The
store uses the same dispatch, history, middleware, profile, and patch path as ordinary graph edits.

## Layout Engines

`jellyflow-layout` keeps automatic layout outside the core document model. Layout engines receive a
graph projection and return `LayoutResult` or normal `GraphTransaction` values that hosts can apply,
animate, or discard.

Built-in engines:

- `dugong`: layered DAG layout.
- `tidy_tree`: tree layout that centers parents over children for hierarchy and outline views.
- `mind_map_radial`: radial mind-map layout.
- `mind_map_freeform`: overlap-resolving freeform mind-map layout that respects pinned nodes.

Custom engines implement `LayoutEngine` and register with `LayoutEngineRegistry`. Runtime callers
can build a `LayoutContext` from store measurements and binding pins through
`NodeGraphStore::layout_context()` or `NodeGraphStore::layout_context_with_binding_pins()`.
For common cases, `LayoutPresetBuilder` builds ordinary `LayoutEngineRequest` values for workflow,
tree, radial mind-map, and freeform mind-map layouts, so hosts can start from a preset and still
override engine IDs, direction, spacing, scope, and measured sizes.

```sh
cargo run -p jellyflow-runtime --example layout_engines
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
DUGONG_DAGREISH_TIMING=1 cargo run -p jellyflow-layout --example dugong_timing
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
`release-preflight` is a manual publish-readiness check, `release-crates` publishes the four crates
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

- Jellyflow is headless. It does not render nodes, edges, handles, panels, menus, or accessibility
  text.
- It does not own raw pointer capture, keyboard focus policy, DOM/React provider state, browser
  measurement APIs, GPU resources, screenshot tests, or pixel comparisons.
- `0.1.x` is the initial public line. Public APIs are intentionally small but may still change as
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

## Links

- Changelog: [`CHANGELOG.md`](CHANGELOG.md)
- Publishing guide: [`docs/releasing/CRATES_IO.md`](docs/releasing/CRATES_IO.md)
- Performance baseline: [`docs/reviews/runtime-performance-baseline-2026-06-12.md`](docs/reviews/runtime-performance-baseline-2026-06-12.md)
- Adapter boundary ADR: [`docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`](docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md)
- Layout extension ADR: [`docs/adr/0005-layout-engine-extension-boundary.md`](docs/adr/0005-layout-engine-extension-boundary.md)
- License: dual MIT or Apache-2.0; see [`LICENSE-MIT`](LICENSE-MIT) and
  [`LICENSE-APACHE`](LICENSE-APACHE)
