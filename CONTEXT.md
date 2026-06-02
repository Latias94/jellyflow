# Jellyflow Project Context

Last updated: 2026-06-02

This file is the high-signal navigation context for agents working in this repository. It does not
replace ADRs, closeout audits, or workstream evidence. When there is a conflict, accepted ADRs
outrank this file, and workstream evidence outranks session memory.

## Product Intent

Jellyflow is a headless Rust node/flow graph engine extracted from Fret. It owns the portable graph
document model, transactions, runtime rules, schema/profile pipeline, view state, store primitives,
renderer-neutral interaction helpers, geometry, and reusable conformance fixtures.

The reference product is `repo-ref/xyflow`. Jellyflow should preserve XyFlow-like editor feel at the
headless runtime boundary: graph operation semantics, store commit ordering, interaction policy,
viewport behavior, gesture lifecycle callbacks, node/edge change projections, and geometry/hit-test
contracts. It should not copy XyFlow's DOM, React, or renderer architecture.

## Crate Boundaries

- `jellyflow-core` owns stable IDs, the serializable graph document model, type descriptors,
  interaction-policy value types, and undoable graph operations/transactions.
- `jellyflow-runtime` owns headless I/O payloads, `NodeGraphStore`, rules, schema/profile hooks,
  policy resolution, fit-view math, selection, node drag, viewport pan/zoom, auto-pan, geometry,
  XyFlow-compatible projections, and conformance fixtures.
- `templates/headless-adapter` is a non-workspace external adapter skeleton that proves third-party
  consumers can run conformance checks without Fret or renderer dependencies.
- Future adapter crates such as `jellyflow-wgpu`, `jellyflow-egui`, or `jellyflow-fret` may own
  windowing, input capture, renderer integration, screenshot checks, and pixel tests.

`jellyflow-core` and `jellyflow-runtime` must stay free of Fret UI, renderer, platform, `wgpu`, and
`winit` dependencies. Use the dependency smoke scripts when changing manifests or public boundaries.

## ADR Index

- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`: Jellyflow is the reusable
  engine boundary; Fret-specific UI and compatibility wrappers belong outside the headless crates.
- `docs/adr/0002-jellyflow-model-policy-boundary.md`: v1 keeps persisted policy/layout/presentation
  fields in `Graph`; effective interaction policy is resolved through `runtime::policy`; schema
  migration is deferred until a versioned migration plan exists.
- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`: XyFlow feel is tested through
  layered headless contracts first; renderer smoke tests belong in adapter crates, not the runtime.

## Runtime Interaction Strategy

Adapters should translate platform input into renderer-neutral Jellyflow calls, then validate the
runtime traces before adding renderer smoke tests.

- Use `runtime::selection` and `NodeGraphStore::apply_selection_box` for canvas-space selection.
- Use `runtime::drag`, `NodeGraphStore::plan_node_drag`, and `NodeGraphStore::apply_node_drag` for
  deterministic node drag behavior.
- Use `runtime::viewport` and store viewport helpers for drag-pan and zoom-around-pointer behavior.
- Use `runtime::auto_pan` and `NodeGraphStore::apply_auto_pan` for deterministic auto-pan frames.
- Use `runtime::geometry` for handle endpoints, edge paths, and numeric hit testing.
- Use `runtime::policy` for effective node, port, and edge interaction policy.
- Use `runtime::xyflow` only for XyFlow-shaped compatibility vocabulary: node/edge changes,
  callback adapters, and controlled-mode projections.
- Use `runtime::conformance` for durable pre-render adapter fixtures and suite reports.

`ConformanceAction::dispatch_transaction` is a low-level graph fixture escape hatch. Prefer
interaction-specific actions for adapter feel fixtures.

## Testing And Validation

Prefer the smallest meaningful gate for local edits, and record fresh evidence before closing a
workstream.

Common gates:

```text
cargo fmt --check
cargo check --workspace
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 tools/check_no_fret_dependencies.py
python3 tools/check_external_consumer_smoke.py
git diff --check
```

Adapter-facing gates:

```text
cargo run -p jellyflow-runtime --example conformance_harness -- check <fixture-dir>
cargo run -p jellyflow-runtime --example conformance_harness -- approve <fixture-dir>
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

Do not add `wgpu`, egui, Fret, screenshot, or pixel dependencies to `jellyflow-core` or
`jellyflow-runtime` to prove headless behavior.

## Workstream State

Workstreams live under `docs/workstreams/`. They own durable lane evidence, task ledgers, closeout
audits, and handoffs. At this update, the active workstream is
`docs/workstreams/jellyflow-node-drag-parent-expansion-v1/`, which deepens runtime node drag by
planning parent group expansion for XyFlow-like `expandParent` behavior. Verify current state with:

```text
for f in docs/workstreams/*/WORKSTREAM.json; do jq -r '[input_filename, .status] | @tsv' "$f"; done
```

Open a new workstream for medium or cross-crate changes. Use a direct task for small documentation
or local refactor cleanups.

Closed lane themes include:

- runtime public-surface cleanup and XyFlow compatibility isolation;
- model policy boundary and `runtime::policy`;
- geometry/spatial extraction into renderer-neutral runtime APIs;
- interaction harness, node drag, viewport, and auto-pan kernels;
- conformance module splits, fixture format, golden approval, CLI harness, fixture discovery, and
  file-backed fixture loading;
- adapter conformance suite runner and copyable headless adapter template;
- runtime test surface/module split cleanup.

## Refactor Guardrails

Preserve intentional compatibility surfaces that support adapter behavior:

- keep `runtime::xyflow` as the explicit XyFlow-compatible projection and callback home;
- keep low-level conformance transaction dispatch as a documented fixture escape hatch;
- keep renderer/platform work outside the headless crates.

Good fearless-refactor candidates are accidental pass-through modules, stale workstream metadata,
duplicated pure runtime math, private modules with too-wide public surfaces, and test fixtures that
can move from implementation detail tests to public contract tests.

Do not move persisted fields out of `Graph` without a new ADR-backed schema migration plan.

## Likely Follow-On Lanes

- Continue `jellyflow-node-drag-parent-expansion-v1`: parent expansion/group resizing behavior for
  node drag.
- Selection-specific auto-pan policy only after integration evidence proves the generic kernel is
  insufficient.
- Real spatial indexing behind `NodeGraphSpatialIndexTuning` after adapter workloads show linear
  scans are too slow.
- Delete planner ownership if runtime should plan deletion eligibility instead of leaving it to
  adapters.
- Renderer smoke harnesses in future adapter crates.
- Schema migration only after policy facade usage proves which persisted fields should leave
  `jellyflow_core::core::Graph`.
