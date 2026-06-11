# ADR 0005: Layout Engine Extension Boundary

Status: Accepted
Date: 2026-06-11

## Context

Jellyflow now has an optional `jellyflow-layout` crate that can run a Dagre-compatible layout
through `dugong`. That adapter already follows the right headless shape: it projects a
`jellyflow_core::core::Graph`, produces a `LayoutResult`, and turns node moves into a normal
`GraphTransaction`. `jellyflow-runtime` exposes thin store helpers that dispatch the resulting
transaction through the existing runtime pipeline.

Host products still need more than one algorithm. Diagram editors, mind-map canvases, image
annotation workspaces, and freeform organization tools often need product-specific placement
heuristics. Those heuristics affect product feel, but they should not force Jellyflow to depend on
renderer frameworks, DOM APIs, Fret UI, `wgpu`, `winit`, or a specific external layout engine.

The risk is over-generalizing too early. A broad public registry, multiple default algorithms, and
custom capabilities can freeze the wrong API before enough engines have pressured the protocol.
The first extension slice therefore needs a small boundary that proves engine selection and runtime
dispatch with `dugong` before adding mind-map, radial, freeform, or compound engines.

## Decision

Keep layout algorithms in `jellyflow-layout`. Do not move automatic layout into
`jellyflow-core`, and do not make layout engines mutate `Graph` or `NodeGraphStore` directly.

Introduce a minimal layout engine boundary:

- stable engine identifiers for built-in and host-provided engines;
- a small engine trait that receives graph data plus explicit layout context and returns
  `LayoutResult`;
- a deterministic registry owned by the caller, not by global mutable state and not by
  `NodeGraphStore`;
- duplicate-engine and missing-engine errors with stable behavior;
- compatibility wrappers for existing `dugong` functions and runtime methods.

Runtime store helpers may build layout context from runtime-owned measurements and resolved
interaction defaults. Those helpers still apply results by dispatching ordinary
`GraphTransaction` values through the store pipeline. Adapter-reported measurements remain runtime
facts and must not be persisted into `Graph`.

`dugong` remains the first built-in engine. Mind-map, radial, freeform relaxation, compound, COSE,
FCoSE, Graphviz, ELK, and D3-style engines are deferred until follow-up work provides
representative graph fixtures and a build-vs-use comparison.

Future native or ported engines must pass a decision gate before becoming default capabilities.
That gate should compare external references and native implementations on dependency weight,
license, determinism, performance, tuning control, platform/WASM constraints, maintenance cost,
and behavior quality on representative graphs.

## Consequences

- `jellyflow-core` stays a headless storage and operation substrate.
- `jellyflow-layout` becomes the optional algorithm layer and can grow without changing graph
  schema.
- `jellyflow-runtime` remains a facade over store-owned measurements, layout context, and dispatch.
- Host products can provide custom layout behavior without forking Jellyflow, while keeping registry
  ownership explicit.
- Existing `dugong` users keep the named compatibility functions during the transition.
- Additional default engines are not accepted by preference alone; they need fixture evidence and a
  documented build-vs-use decision.

## Follow-Up

- Add the minimal layout engine protocol and registry to `jellyflow-layout`.
- Move the existing `dugong` adapter behind the protocol while preserving compatibility wrappers.
- Add runtime facades that accept an explicit registry and build context from store measurements.
- Add external headless smoke coverage for the generic layout path.
- Plan mind-map, radial, and freeform engines separately after representative validation fixtures
  exist.

## Evidence

- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/adr/0002-jellyflow-model-policy-boundary.md`
- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `docs/adr/0004-resize-containment-and-lifecycle-boundary.md`
- `crates/jellyflow-layout/src/lib.rs`
- `crates/jellyflow-runtime/src/runtime/layout.rs`
- `crates/jellyflow-runtime/src/runtime/measurement.rs`
