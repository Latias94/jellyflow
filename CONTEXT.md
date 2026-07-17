# Jellyflow Project Context

Last updated: 2026-07-17

This file is the high-signal navigation context for agents working in this repository. Shipped code
and executable tests define current behavior. The active native-host refactor plan records the
target architecture and acceptance contract; research records supporting evidence.

## Product Intent

Jellyflow is a headless Rust node/flow graph engine extracted from Fret. It owns portable graph
documents, revisioned commands, graph policies, persistence, and renderer-neutral protocol
contracts.
Framework adapters own native node component trees, input, focus, layout, and measurement.

The reference product is `repo-ref/xyflow`. Jellyflow should preserve XyFlow-like editor behavior
at the headless boundary without copying its DOM, React, or renderer architecture. The active target
is native Dify- and Shader Graph-class node authoring: portable graph facts and an owned view protocol,
with Open GPUI as the first native host and other frameworks supported through independent adapters.

## Crate Boundaries

- `jellyflow-core` owns stable IDs, the canonical in-memory graph model, validation, type
  descriptors, interaction-policy value types, and mutation footprints. It has no raw Serde graph
  representation or public operation journal.
- `jellyflow-runtime` exposes an authoritative `GraphRuntime` plus independent `ViewSession`s.
  The runtime owns revisioned graph commands, the private reversible mutation/history kernel,
  persistence, and renderer-neutral graph facts; sessions own transform, selection, geometry,
  routing, culling, and frame/feedback state.
- `jellyflow-open-gpui` is the first real native adapter. It may depend on Open GPUI and owns the
  retained registry, component lifecycle, measurement bridge, and input arbitration.
- `jellyflow-egui` is the second native adapter and conformance witness. Future adapters consume the
  same owned protocol but keep their framework types and widget lifecycles local.

`jellyflow-core`, `jellyflow-layout`, and `jellyflow-runtime` must stay free of UI framework,
windowing, focus, IME, popup, and platform dependencies. Use dependency smoke checks when changing
manifests or public boundaries.

## Current Architecture

The active architecture replaces the former portable UI DSL and node-kit direction. Its contracts
live in public types, versioned protocol and migration tests, and the 2026-07-12 native-host plan.
`docs/adr/` was intentionally removed rather than maintained as a parallel registry. Do not restore
deleted galleries, standalone native-smoke glue, or obsolete examples merely because historical
plans mention them. Real-window smoke belongs in the single flagship product through `--smoke` and
`--workflow --smoke`.

## Runtime Interaction Strategy

Adapters consume an owned render frame, render ordinary framework-native components, collect
complete node and real-port geometry after layout, append revisioned commands and layouts to the
frame's ordered feedback log, then submit it once. They must not expose framework widgets through
the headless crates or recreate a cross-framework UI DSL.

- Use `GraphRuntime` for authoritative graph snapshots, revisioned commands, history, and fan-out.
- Attach one `ViewSession` per native canvas; transform, selection, geometry, routing, and frame
  admission remain view-local.
- Use `GraphCommand::set_node_geometry` for atomic multi-node drag, resize, and layout commits.
- Use `ViewSession::evaluate_connection`, `port_connection_facts`, and
  `create_from_wire_candidates` for authoritative connection UI decisions.
- Use `FrameRequest`, `FeedbackCollector`, and complete real-port `NodeLayout` reports for native
  frame and measurement integration.
- Use `runtime::geometry` for handle endpoints, edge paths, and numeric hit testing.
- Use `GraphRuntimePolicy` for authoritative connection, deletion, and reconnection fallbacks.
  Framework-local selection, focus, drag, gesture, and hit-target policy stays in each adapter.

## Testing And Validation

Prefer the smallest meaningful gate for local edits, and record fresh evidence before closing a
workstream.

Common gates:

```text
cargo fmt --check
cargo check --workspace
cargo nextest run --workspace
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets -- -D warnings
python3 tools/check_no_fret_dependencies.py
python3 -m unittest discover -s tools/tests -p "test_*.py"
python3 tools/check_no_legacy_ui_surfaces.py
python3 tools/check_external_consumer_smoke.py
git diff --check
```

Adapter-facing gates:

```text
cargo nextest run --manifest-path templates/native-adapter/Cargo.toml
cargo nextest run -p jellyflow-open-gpui
cargo nextest run -p jellyflow-egui
cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked --features real-window-smoke -- --smoke
cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked --features real-window-smoke -- --workflow --smoke
```

Do not add `wgpu`, egui, Fret, screenshot, or pixel dependencies to `jellyflow-core` or
`jellyflow-runtime` to prove headless behavior.

## Refactor Guardrails

Preserve only contracts exercised by current tests or required by the active Product Contract. The
refactor is allowed to break public Rust APIs and delete former UI DSL surfaces, but graph-document
migration must be explicit, lossless where supported, and rejection-based where not.

`GraphFile` writes envelope V2. Released Jellyflow 0.3 V1 documents are an intentional
incompatibility boundary and fail with structured `UnsupportedEnvelopeVersion` before payload
decoding; no V1 compatibility alias or lossy focusability migration is supported.

Do not add compatibility facades merely to preserve stale `NodeGraphStore`, semantic descriptor, or
adapter-planner APIs. Keep renderer/platform work outside headless crates and prove the replacement
through protocol, persistence, conformance, and native-adapter tests.
