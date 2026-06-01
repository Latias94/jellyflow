# Jellyflow Geometry Spatial v1

Status: Closed
Last updated: 2026-06-01

## Why This Lane Exists

Jellyflow is a headless Rust node/flow graph engine intended to serve self-rendered adapters such
as Fret, egui, and other custom canvas frameworks. The core/runtime split is now stable enough to
compile and test independently, but geometry-related behavior is still scattered across runtime
helpers, fit-view math, lookup maps, and XyFlow reference vocabulary.

XyFlow's `packages/system` owns reusable non-React behavior such as graph utilities, edge path
math, drag helpers, pan/zoom helpers, handle positioning, visible-element filtering, and resize
math. Jellyflow should not port DOM or d3 bindings, but it does need the headless math and spatial
contracts that multiple Rust adapters would otherwise reimplement differently.

## Relevant Authority

- ADRs:
  - `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
  - `docs/adr/0002-jellyflow-model-policy-boundary.md`
- Existing workstreams:
  - `docs/workstreams/jellyflow-runtime-public-surface-v1/`
  - `docs/workstreams/jellyflow-model-policy-boundary-v1/`
- Reference implementation:
  - `repo-ref/xyflow/packages/system/src/utils/`
  - `repo-ref/xyflow/packages/system/src/xydrag/`
  - `repo-ref/xyflow/packages/system/src/xyhandle/`
  - `repo-ref/xyflow/packages/system/src/xypanzoom/`
  - `repo-ref/xyflow/packages/system/src/xyresizer/`
- Primary code:
  - `crates/jellyflow-runtime/src/runtime/fit_view/`
  - `crates/jellyflow-runtime/src/runtime/utils/`
  - `crates/jellyflow-runtime/src/runtime/lookups/`
  - `crates/jellyflow-runtime/src/io/tuning/spatial_index.rs`
  - `crates/jellyflow-runtime/src/io/config/interaction/config.rs`

## Problem

Jellyflow currently has useful geometry pieces, but not a deep geometry module:

- `runtime::utils::geometry` and `runtime::fit_view::geometry` each define private bounds logic.
- `NodeGraphLookups` stores node, edge, and connection maps, but not parent lookup, absolute layout,
  handle bounds, edge endpoint geometry, z-order, visible-element state, or a spatial query index.
- `NodeGraphInteractionConfig` already exposes geometry-adjacent tuning such as connection radius,
  reconnect radius, edge interaction width, bezier hit-test steps, snap grid, snaplines, and spatial
  index tuning, but several of those fields do not yet back shared headless behavior.
- Rust adapters that need node dragging, edge routing, hit testing, minimap, selection boxes, or
  visible-element filtering would currently have to duplicate behavior outside Jellyflow.

## Target State

- A single headless geometry module owns reusable canvas bounds, viewport transforms, node rect
  resolution, and deterministic spatial queries.
- Fit-view and public runtime utility helpers use the same geometry primitives.
- `NodeGraphLookups` or an adjacent derived-layout index exposes parent/child lookup and layout
  queries without requiring adapters to scan graph maps manually.
- Edge endpoint and edge path math is available as renderer-neutral data, not SVG- or DOM-specific
  output.
- Spatial-index tuning has a real implementation or is explicitly downgraded to deferred config.
- All behavior stays free of Fret UI, renderer, platform, `wgpu`, `winit`, DOM, and d3 dependencies.

## In Scope

- Consolidating duplicated bounds/viewport math.
- Adding a headless `runtime::geometry` module or equivalent internal module structure.
- Moving existing `runtime::utils` and `fit_view` helpers onto shared primitives without changing
  their public behavior unnecessarily.
- Adding derived layout data needed by Rust adapters: node bounds, parent lookup, visible-node
  queries, and deterministic spatial lookup behavior.
- Adding renderer-neutral edge endpoint/path primitives when the input data is already headless.
- Adding tests that compare old helper behavior and cover edge cases: hidden nodes, missing size,
  node origin, parent-relative positions, non-finite values, and deterministic ordering.

## Out Of Scope

- DOM event binding, d3 integration, React/Svelte behavior, or browser-specific pointer handling.
- Full drag, pan/zoom, resize, and selection gesture state machines. Those should follow after the
  geometry substrate is stable.
- Moving persisted layout or policy fields out of `jellyflow_core::core::Graph`.
- Creating a new published crate before the module contract has survived runtime tests.
- Renderer output APIs tied to a single backend, such as SVG path strings as the only edge shape.

## Starting Assumptions

| Assumption | Confidence | Evidence | Consequence if wrong |
| --- | --- | --- | --- |
| Geometry/spatial work is the next best fearless-refactor lane. | High | ADR 0001 names future `jellyflow-geometry`; previous workstreams defer geometry extraction. | If adapters do not share this behavior, keep it inside `jellyflow-runtime` and avoid a crate split. |
| A module-first refactor is safer than immediately creating a third crate. | High | Only `jellyflow-core` and `jellyflow-runtime` exist today; publishing is blocked. | If external users need an independent crate now, add an ADR before splitting packages. |
| Existing fit-view and bounds helpers are the smallest proof slice. | High | They already have tests and duplicate private bounds math. | If they cannot share primitives cleanly, stop after documenting the contract and split a narrower task. |
| Interaction kernels should wait until geometry is stable. | High | XyFlow interaction helpers depend on node bounds, absolute positions, handles, and transforms. | If Fret/egui need gestures immediately, open a separate interaction workstream after JGS-020. |

## Source Coverage Audit

| Source | State | Evidence path | Impact | Required action |
| --- | --- | --- | --- | --- |
| User goal and constraints | COVERED | Conversation plus this DESIGN.md | Confirms headless Rust adapters are the target. | None. |
| ADR 0001 | COVERED | `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md` | Defines headless boundary and future geometry package direction. | None. |
| ADR 0002 | COVERED | `docs/adr/0002-jellyflow-model-policy-boundary.md` | Prevents premature persisted schema movement. | None. |
| Closed workstreams | COVERED | `docs/workstreams/jellyflow-runtime-public-surface-v1/`, `docs/workstreams/jellyflow-model-policy-boundary-v1/` | Shows public surface and policy facade are already handled. | None. |
| Architecture lane maps | OUT_OF_SCOPE | `docs/architecture/` absent | This repo has no architecture lane map yet; this workstream is self-contained. | Add only if future multi-lane coordination starts. |
| XyFlow reference | COVERED | `repo-ref/xyflow/packages/system/src/` | Gives source concepts for headless math without DOM bindings. | Use as behavior reference, not as a direct port mandate. |
| Current code/tests | COVERED | `crates/jellyflow-runtime/src/runtime/{fit_view,utils,lookups}/` | Identifies the first proof slice and validation commands. | None. |

## Architecture Direction

Start with a runtime-internal geometry module, not a new crate. The first proof should remove
duplicated bounds logic while preserving current public helper behavior. Once two or more adapters
need the same stable contract, the module can be promoted to `jellyflow-geometry` with a small
public surface.

The direction is:

- `jellyflow-core` remains storage and undoable graph edits.
- `jellyflow-runtime::runtime::geometry` owns shared math and spatial queries.
- `jellyflow-runtime::runtime::lookups` owns graph-derived maps and can delegate layout queries to
  geometry primitives.
- `runtime::xyflow` remains compatibility projection, not the canonical geometry home.
- Adapter crates own renderer integration and concrete input-event binding.

## Closeout Condition

This lane can close when:

- duplicated bounds/viewport math has a single implementation,
- fit-view and public runtime utilities use the shared geometry substrate,
- the derived lookup/spatial query story is implemented or explicitly split,
- edge endpoint/path primitives are either shipped or split into a named follow-on,
- validation gates pass with fresh evidence,
- docs and examples reflect the shipped module boundary,
- and any interaction-kernel work is deferred to a separate workstream.
