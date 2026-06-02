# Jellyflow Node Resize Kernel v1

Status: Closed
Last updated: 2026-06-02

## Why This Lane Exists

Jellyflow now has renderer-neutral node drag planning, parent expansion, selection, viewport,
auto-pan, geometry, and conformance fixtures. Node resizing is the next obvious XyFlow-like editor
feel gap: the model and transaction layer already expose `Node.size` and `GraphOp::SetNodeSize`,
but there is no runtime resize planner that adapters can drive without reimplementing XyFlow's
`XYResizer` math.

XyFlow keeps much of resize calculation in `packages/system/src/xyresizer`, separate from React UI
controls. That makes resize a good headless-runtime candidate for Jellyflow, while DOM drag
bindings, resize handles, cursor styling, and rendered controls remain adapter responsibilities.

## Relevant Authority

- `CONTEXT.md`
- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/adr/0002-jellyflow-model-policy-boundary.md`
- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `docs/workstreams/jellyflow-node-drag-kernel-v1/`
- `docs/workstreams/jellyflow-node-drag-parent-expansion-v1/`
- `repo-ref/xyflow/packages/system/src/xyresizer/`
- `repo-ref/xyflow/packages/react/src/additional-components/NodeResizer/`

## Problem

Adapters can set node sizes through low-level graph transactions or XyFlow-shaped node changes, but
they currently have to invent resize behavior themselves:

- resize control direction and affected axes;
- min/max size constraints;
- node origin and left/top position changes;
- snap-grid corrected pointer positions;
- parent extent constraints;
- child extent constraints when resizing a parent;
- keep-aspect-ratio behavior;
- parent expansion interaction;
- conformance vocabulary for resize traces.

That is shallow for a headless node library: Jellyflow stores the data and ops, but the behavior
callers actually need still lives outside the runtime.

## Target State

This lane closes when Jellyflow has a renderer-neutral node resize planning contract:

- pure runtime request/plan types under a focused resize module;
- deterministic `GraphTransaction` output using existing reversible ops such as `SetNodeSize`,
  `SetNodePos`, and `SetGroupRect` when applicable;
- min/max, node-origin, control-direction, and basic extent behavior covered by focused tests;
- parent/child extent behavior either implemented or explicitly split with evidence;
- conformance/template coverage for adapter-facing resize traces when the public contract settles;
- no DOM, d3, renderer, `ResizeObserver`, `wgpu`, egui, Fret, screenshot, or pixel dependency in
  the headless crates.

## In Scope

- Runtime-owned resize request, plan, direction, and item vocabulary.
- Planning node size and optional node position changes through normal graph transactions.
- Existing `Node.size`, `Node.origin`, `Node.extent`, `Node.expand_parent`, groups, and policy
  resolution where applicable.
- Focused runtime tests first, then conformance/template coverage after the interface stabilizes.
- README/runtime README closeout guidance.

## Out Of Scope

- DOM drag handles, cursor classes, `ResizeObserver`, rendered controls, or React/Svelte UI.
- Raw pointer capture and browser event normalization.
- Renderer smoke, screenshots, or pixel tests inside `jellyflow-core` or `jellyflow-runtime`.
- Schema migration or moving persisted sizing/policy fields out of `Graph`.
- Exact XyFlow `NodeResizer` component API parity before a Rust adapter needs it.

## Starting Assumptions

| Assumption | Confidence | Evidence | Consequence if wrong |
| --- | --- | --- | --- |
| Resize planning belongs in `jellyflow-runtime`. | High | `XYResizer` has framework-independent math and Jellyflow owns interaction helpers. | Adapters would duplicate min/max, origin, and extent behavior. |
| Existing `SetNodeSize` and `SetNodePos` ops are enough for initial resize plans. | High | Core already supports reversible size and position ops. | Add a new op only if transaction evidence proves a missing edit. |
| Start with pure planner tests before conformance/schema changes. | High | Drag and viewport lanes stabilized public contracts this way. | Avoid fixture churn while resize names are still settling. |
| Parent/child extent and keep-aspect-ratio behavior need separate slices. | High | XyFlow `getDimensionsAfterResize` is dense and constraint-heavy. | Prevent the first task from becoming a broad port. |

## Architecture Direction

Deepen the runtime around a small planner interface:

```text
adapter resize intent
  -> runtime resize planner
  -> GraphTransaction(SetNodeSize..., optional SetNodePos/SetGroupRect...)
  -> NodeGraphStore trace and conformance fixtures
```

Adapters should provide already-normalized canvas-space resize intent and own handle UI. The
runtime should own deterministic geometry and transaction planning.

## Source Coverage

| Source | State | Evidence | Impact |
| --- | --- | --- | --- |
| User goal | COVERED | Continue XyFlow-feel headless architecture work | Resize is a missing interaction kernel. |
| `CONTEXT.md` | COVERED | Runtime interaction strategy and follow-ons | Confirms renderer-free runtime boundary. |
| ADR 0001 | COVERED | Headless engine boundary | Keeps UI/rendering outside this lane. |
| ADR 0002 | COVERED | Persisted size/policy fields stay in `Graph` for v1 | Reuse `Node.size`, `Node.extent`, and `Node.expand_parent`. |
| ADR 0003 | COVERED | Layered headless testing before renderer smoke | Guides runtime/conformance/template coverage. |
| XyFlow `xyresizer` | COVERED | Source has framework-independent resize math | Primary behavior reference. |
| Current Jellyflow ops | COVERED | `SetNodeSize`, `SetNodePos`, and `SetGroupRect` exist | Avoid new transaction ops in early slices. |
| Renderer adapters | OUT_OF_SCOPE | Future adapter crates | Renderer smoke remains outside runtime. |

## Refactor Brief

- **Intent**: move resize behavior from hypothetical adapters into a deep runtime planner.
- **Scope**: runtime resize module, focused tests, conformance/template after interface stability,
  and closeout docs.
- **Deletion plan**: no compatibility fields are deleted; the lane removes the need for adapter-side
  ad hoc resize math.
- **Boundary plan**: runtime owns planning; adapters own raw input, handle UI, renderer, and pixels.
- **Testing plan**: use public store/planner seams, not private helper-only assertions.
- **Risk plan**: split parent/child extent or exact keep-aspect-ratio parity if it broadens the
  first planner slice.
- **Workflow plan**: durable workstream with vertical tasks and autonomous commits after verified
  slices.
- **Scale plan**: medium architecture lane.

## Closeout Condition

This lane can close when:

- resize planning is implemented or explicitly split into accepted follow-ons;
- focused runtime tests prove the shipped planner behavior;
- conformance/template coverage proves adapter-facing traces when needed;
- docs teach the runtime/adapter split;
- fresh package, clippy, JSON, and diff gates pass;
- renderer and platform follow-ons are split or deferred.

Closed on 2026-06-02. Target-size resize planning, direction/origin handling, conformance coverage,
template coverage, and runtime/adapter documentation are complete. Exact pointer-resize extent and
keep-aspect-ratio parity is split until adapter evidence requires a pointer-session request.
