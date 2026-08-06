# Jellyflow Project Context

Last updated: 2026-08-04

This file is the high-signal navigation context for agents working in this repository. Shipped code
and executable tests define current behavior. The adapter-authoring guide records the current
ownership boundary; plans and research are historical implementation evidence.

## Product Intent

Jellyflow is a headless Rust node/flow graph engine extracted from Fret. It owns portable graph
documents, version-checked commands, graph policies, persistence, and renderer-neutral protocol
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
  The runtime owns opaque-version graph commands, the private reversible mutation/history kernel,
  persistence, and renderer-neutral graph facts; sessions own transform, selection, geometry,
  routing, culling, and frame/feedback state.
- `jellyflow-open-gpui` is the first real native adapter. It may depend on Open GPUI and owns the
  retained registry, component lifecycle, measurement bridge, and input arbitration.
- `jellyflow-egui` is the second native adapter and an independent immediate-mode witness. Future
  adapters consume the same owned protocol but keep their framework types and widget lifecycles
  local.

`jellyflow-core`, `jellyflow-layout`, and `jellyflow-runtime` must stay free of UI framework,
windowing, focus, IME, popup, and platform dependencies. Use dependency smoke checks when changing
manifests or public boundaries.

## Current Architecture

The active architecture keeps graph semantics in direct core/runtime dependencies and native UI in
independent adapters. Its contracts live in public types, runtime owner tests, adapter-local native
tests, and migration tests. Real-window evidence belongs to the dedicated
`open-gpui-real-window-smoke` binary, which is excluded from ordinary product builds. CI runs five
representative processes: Shader compact at zoom `0.5` and `2`, Workflow compact at zoom `1`,
Shader normal at zoom `1`, and one directed Shader LOD-threshold run. The Rust harness owns live UI
semantics; Python validates only process transport, run identity, listed artifacts, and the atomic
`results.json` matrix result.

## Runtime Interaction Strategy

Adapters consume an owned render frame, render ordinary framework-native components, collect
complete node and real-port geometry after layout, append version-checked commands and layouts to the
frame's ordered feedback log, then submit it once. They must not expose framework widgets through
the headless crates or recreate a cross-framework UI DSL.

- Use `GraphRuntime` for authoritative graph snapshots, version-checked commands, history, and fan-out.
- Attach one `ViewSession` per native canvas; transform, selection, geometry, routing, and frame
  admission remain view-local.
- Use `GraphCommand::set_node_geometry` for atomic multi-node drag, resize, and layout commits.
- Use `ViewSession::evaluate_connection`, `port_connection_facts`, and
  `create_from_wire_candidates` for authoritative connection UI decisions.
- Use `FrameRequest`, `FeedbackCollector`, and complete real-port `NodeLayout` reports for native
  frame and measurement integration.
- Use `runtime::geometry` for handle endpoints, edge paths, and numeric hit testing.
- Use `GraphRuntimePolicy` for authoritative connection, deletion, and reconnection fallbacks.
  Raw framework input, focus, pointer capture, gesture recognition, and hit-target mechanics stay in
  each adapter. Normalized selection and drag decisions and their state transitions stay in the
  runtime interaction session.

## Testing And Validation

Prefer the smallest meaningful gate for local edits, and record fresh evidence before closing a
workstream.

Common gates:

```text
cargo fmt --all -- --check
cargo check --workspace --locked
cargo nextest run --workspace --locked --cargo-quiet
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 tools/check_no_fret_dependencies.py
python3 -m unittest discover -s tools/tests -p "test_*.py"
python3 tools/check_external_consumer_smoke.py
git diff --check
```

Adapter-facing gates:

```text
cargo nextest run -p jellyflow-open-gpui --locked
cargo nextest run -p jellyflow-egui --locked
cargo test --manifest-path examples/canvas-jellyflow/Cargo.toml --workspace --doc --locked
cargo build --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --bin open-gpui-real-window-smoke \
  --locked --features real-window-smoke
python3 tools/run_real_window_smoke_matrix.py \
  --binary examples/canvas-jellyflow/target/debug/open-gpui-real-window-smoke \
  --capture-root target/real-window-smoke \
  --timeout-seconds 60
```

Do not add `wgpu`, egui, Fret, screenshot, or pixel dependencies to `jellyflow-core` or
`jellyflow-runtime` to prove headless behavior.

## Refactor Guardrails

Preserve only contracts exercised by current tests or required by the active Product Contract. The
refactor is allowed to break public Rust APIs and delete former UI DSL surfaces, but graph-document
migration must be explicit, lossless where supported, and rejection-based where not.

`GraphFile` writes envelope V2. Released Jellyflow 0.3 V1 documents are an intentional
incompatibility boundary and fail with structured `UnsupportedEnvelopeVersion` before payload
decoding; no lossy focusability migration is supported.

Do not add broad re-export layers merely to preserve stale APIs. Keep renderer/platform work outside
headless crates and prove boundaries through protocol, persistence, direct external consumers, and
native adapter tests.
