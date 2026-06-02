# Jellyflow

Jellyflow is a headless Rust node/flow graph engine extracted from Fret. It owns the portable graph
document model, transactions, rules, schema/profile pipeline, view state, and store primitives that
can be used without Fret UI, renderer, platform, or windowing crates.

The initial package split is intentionally small:

- `jellyflow-core`: graph IDs, document model, type descriptors, interaction value types, and
  undoable graph transactions.
- `jellyflow-runtime`: headless `NodeGraphStore`, view-state/config payloads, policy resolution,
  rules, schema/profile pipeline, explicit `runtime::xyflow` compatibility projections,
  persistence file types without project-path policy, fit-view math, renderer-neutral selection
  helpers, renderer-neutral node dragging with parent expansion planning, renderer-neutral viewport
  pan/zoom, renderer-neutral viewport animation planning, renderer-neutral viewport pan inertia
  planning, renderer-neutral auto-pan, renderer-neutral geometry, and public headless conformance
  fixtures.

`fret-node` remains the Fret adapter and compatibility facade in the Fret repository. Jellyflow is
the reusable engine boundary for non-Fret consumers.

## Quick Start

```toml
[dependencies]
jellyflow-core = { path = "crates/jellyflow-core" }
jellyflow-runtime = { path = "crates/jellyflow-runtime" }
```

```rust
use jellyflow_core::{Graph, GraphId};
use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow_runtime::NodeGraphStore;

let graph = Graph::new(GraphId::new());
let store = NodeGraphStore::new(
    graph,
    NodeGraphViewState::default(),
    NodeGraphEditorConfig::default(),
);

assert_eq!(store.graph().nodes.len(), 0);
```

Runnable examples live under the crate example directories:

```text
cargo run -p jellyflow-core --example build_graph
cargo run -p jellyflow-runtime --example store_dispatch
cargo run -p jellyflow-runtime --example geometry_edge
cargo run -p jellyflow-runtime --example conformance_harness -- check <fixture-dir>
cargo run -p jellyflow-runtime --example conformance_harness -- approve <fixture-dir>
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

## Interaction Testing Strategy

Jellyflow keeps XyFlow-feel checks at the headless runtime boundary before renderer smoke tests:

- runtime contracts cover store commits, undo/redo, view state, policy, geometry, and hit testing;
- `runtime::selection` turns canvas-space selection boxes into deterministic selection state
  without a renderer dependency;
- `runtime::drag` turns canvas-space node drag intent into deterministic drag items and normal
  `SetNodePos` transactions, including selection co-dragging, snap-to-grid, extents, node origin,
  parent group expansion through `SetGroupRect`, and renderer-neutral gesture payloads;
- `runtime::viewport` turns normalized drag-pan and zoom-around-pointer intent into deterministic
  viewport transforms, while `NodeGraphStore` publishes those changes through the same view-state
  event path used by direct viewport updates;
- `runtime::viewport` also exposes renderer-neutral viewport animation request/plan/frame types and
  double-click zoom planning, while adapters keep ownership of frame clocks, cancellation policy,
  pointer double-click detection, and actual store commits for sampled frames;
- `runtime::viewport` exposes renderer-neutral viewport pan inertia request/plan/frame types that
  sample adapter-provided logical screen px/s release velocity, while adapters keep ownership of
  release velocity estimation, frame clocks, interruption/cancellation policy, and store commits for
  sampled frames;
- `runtime::auto_pan` turns pointer-edge proximity and elapsed frame time into deterministic
  viewport pan frames, while adapters keep ownership of pointer capture and frame scheduling;
- `runtime::conformance` defines reusable fixture scenarios and a runner that drive a real
  `NodeGraphStore` and compare normalized traces for graph transactions, view changes, gesture
  lifecycle events, and XyFlow compatibility callbacks; adapter crates can group scenarios into
  suites, save them as JSON golden fixtures, discover fixture directories, and explicitly approve
  actual headless traces back into golden files through the `conformance_harness` example for
  aggregate pre-render conformance reports;
- runtime adapter-conformance tests use those fixtures for connect, node drag, node drag parent
  expansion, viewport, viewport animation planning, pan inertia replay, double-click zoom, and
  auto-pan behavior before any renderer-specific smoke tests are written;
- `templates/headless-adapter` is a copyable external adapter skeleton that runs node-drag, node
  drag parent expansion, viewport, viewport animation, and pan inertia conformance with
  `cargo --manifest-path` before adding renderer smoke tests;
- wgpu, egui, Fret, screenshot, or pixel tests belong in future adapter crates that consume the
  public Jellyflow runtime APIs.

## Repository Status

This repository was created by a history-preserving path-filtered extraction from Fret. The source
Fret commit is recorded in `JELLYFLOW_SOURCE_COMMIT.txt`, and the filter command is recorded in
`docs/extraction/EXTRACTION_RECORD_2026-05-30.md`.

Crates.io publishing is intentionally blocked until package metadata, CI, package lists, and
publish dry-runs are verified.

## Validation

```text
cargo check --workspace
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
python3 tools/check_no_fret_dependencies.py
python3 tools/check_external_consumer_smoke.py
```

## License

Jellyflow is distributed under either the MIT license or the Apache License, Version 2.0, at your
option.
