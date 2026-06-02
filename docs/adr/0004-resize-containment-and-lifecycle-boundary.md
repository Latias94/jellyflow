# ADR 0004: Resize Containment and Lifecycle Boundary

Status: Accepted
Date: 2026-06-02

## Context

The XyFlow parity review identified node resize as the largest remaining runtime semantics gap.
Jellyflow has since added a headless pointer resize planner that derives resize transactions from
canvas-space pointer movement, min/max constraints, node origin, aspect-ratio locking, axis filters,
and `NodeExtent::{Rect, Parent}` clamps.

XyFlow's `XYResizer` still has two semantics that do not map directly to the current Jellyflow
model:

- **node-as-parent child correction**: when a parent node is resized from the top or left, XyFlow can
  shift children that have `extent: parent` or `expandParent` so their local coordinates remain
  valid;
- **resize lifecycle callbacks**: XyFlow exposes start/update/end callbacks and `shouldResize`
  gating around a pointer resize session.

Jellyflow's current containment model is different:

- `jellyflow_core::core::Node.parent` is `Option<GroupId>`, not `Option<NodeId>`.
- `Group.rect` is the editor container bounds in canvas space.
- `NodeExtent::Parent` means a node is constrained to its parent group when parent expansion is not
  active.
- `jellyflow-core` and `jellyflow-runtime` must remain renderer-free and must not grow React, DOM,
  Fret UI, `wgpu`, or `winit` concerns.

ADR 0002 also keeps `Graph` as the v1 document shape for semantic data, layout, persisted editor
policy, and presentation fields. Adding node-owned child containment would therefore be a model and
schema decision, not a small resize-planner patch.

## Decision

Do not add XyFlow-style node-owned child containment to Jellyflow v1 solely to copy
`XYResizer` child correction.

Jellyflow v1 resize containment remains group-based:

- `Node.parent: Option<GroupId>` continues to represent editor containment.
- `Group.rect` remains the parent bounds for `NodeExtent::Parent`.
- runtime resize planners may clamp nodes to explicit rects or parent group rects.
- future parent expansion during resize, if added, should mutate group rects through normal graph
  transactions.

Exact XyFlow node-as-parent child correction is deferred. If Jellyflow later needs first-class
node-owned containment, it requires a separate ADR that covers:

- persisted model shape and schema migration;
- relationship to semantic subgraphs and groups;
- transaction semantics for parent resize plus child correction;
- adapter compatibility for XyFlow `parentId`;
- conformance fixtures for parent-local coordinates.

Resize lifecycle callback parity belongs in `jellyflow-runtime`, not `jellyflow-core`:

- renderer and DOM pointer mechanics remain adapter-owned;
- runtime may add headless resize gesture events and conformance actions for start/update/end;
- XyFlow-shaped callback projection should live under `runtime::xyflow` or explicit conformance
  vocabulary;
- lifecycle state must not be persisted in the core graph document.

## Consequences

The current pointer resize implementation stays focused and additive. It can support the existing
Jellyflow group containment model without forcing a schema break.

Jellyflow will not claim exact XyFlow resize child-correction parity until a future node-containment
model decision is made. This is an intentional gap, not an implementation oversight.

Adapters that need XyFlow-like nested nodes have two options:

- map XyFlow parent nodes to Jellyflow groups at the adapter boundary; or
- keep adapter-owned compatibility state until Jellyflow accepts a first-class node-containment ADR.

Resize lifecycle callback work can proceed independently after the pure pointer resize contract is
stable. That follow-up should prove callback ordering through headless conformance fixtures before
any renderer smoke tests.

## Follow-Up

- Add resize start/update/end conformance and `runtime::xyflow` callback projection.
- Decide whether resize should support group parent expansion transactions.
- If required by a real adapter, write a separate node-containment ADR before adding
  node-as-parent child correction.

## Evidence

- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/adr/0002-jellyflow-model-policy-boundary.md`
- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `docs/reviews/xyflow-gap-2026-06-02.md`
- `.trellis/tasks/archive/2026-06/06-02-pointer-resize-parity/research.md`
- `crates/jellyflow-core/src/core/model/node.rs`
- `crates/jellyflow-core/src/core/model/resources.rs`
- `crates/jellyflow-runtime/src/runtime/resize/`
