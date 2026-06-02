# Jellyflow Node Drag Parent Expansion v1

Status: Closed
Last updated: 2026-06-02

## Why This Lane Exists

Jellyflow already has renderer-neutral node drag planning, selected-node co-dragging, snap-grid
behavior, extent clamping, and gesture/conformance traces. The remaining XyFlow-like group-drag
follow-on is automatic parent expansion when a child node is dragged beyond its parent group.

The model already exposes `Node.parent`, `Node.extent`, and `Node.expand_parent`, and the graph
operation layer already supports `GraphOp::SetGroupRect`. Today, however, `NodeExtent::Parent` with
`expand_parent = true` only removes the parent clamp in the drag planner. It does not produce a
parent group resize transaction. Adapters would therefore have to reimplement parent expansion to
get XyFlow-like sub-flow feel.

## Relevant Authority

- `CONTEXT.md`
- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/adr/0002-jellyflow-model-policy-boundary.md`
- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `docs/workstreams/jellyflow-node-drag-kernel-v1/`
- `docs/workstreams/jellyflow-node-drag-module-split-v1/`
- `repo-ref/xyflow/packages/react/src/store/index.ts`
- `repo-ref/xyflow/packages/system/src/utils/store.ts`
- `repo-ref/xyflow/packages/system/src/utils/graph.ts`

## Problem

The current drag module has a shallow `expand_parent` seam:

- policy resolution exposes an effective `expand_parent` flag;
- `NodeExtent::Parent` with `expand_parent = false` clamps movement to the current parent group
  rect;
- `NodeExtent::Parent` with `expand_parent = true` resolves to no extent, so movement is allowed;
- the drag transaction still emits only `SetNodePos` operations.

This is not enough for adapter feel. XyFlow's `expandParent` behavior does more than allow the child
to move outside the current parent bounds: it expands the parent rect, clamps the dragged child to
non-negative parent-local coordinates, and compensates non-dragged siblings when a parent expands to
the left or top.

## Target State

This lane closes when Jellyflow has a renderer-neutral parent expansion contract for node drag:

- pure drag planning can emit both node movement ops and parent group rect ops;
- `expand_parent = false` continues to clamp `NodeExtent::Parent` to the current parent rect;
- `expand_parent = true` expands the parent group rect when a dragged child would exceed it;
- group rect expansion is deterministic for single-node and multi-node drags;
- non-dragged sibling compensation is specified or explicitly split with evidence;
- conformance fixtures can prove the resulting transaction and callback traces;
- no renderer, DOM, resize-handle, pointer-capture, `wgpu`, egui, or Fret dependency enters the
  headless crates.

## In Scope

- Runtime drag planner behavior for child nodes with `parent`, `NodeExtent::Parent`, and
  `expand_parent`.
- `GraphOp::SetGroupRect` planning during drag transactions.
- Deterministic op ordering when a drag moves nodes and expands one or more parent groups.
- Focused runtime tests for single-child expansion, false-policy clamping, multi-selection behavior,
  and sibling compensation decisions.
- Conformance and adapter-template traces if the runtime behavior changes adapter-facing callbacks.
- README/runtime README guidance at closeout.

## Out Of Scope

- Raw pointer capture, DOM selector behavior, drag handles, browser event quirks, or platform input
  code.
- Resize handles and `NodeResizer` parity beyond the drag expansion contract.
- Renderer smoke, screenshot, or pixel tests inside `jellyflow-core` or `jellyflow-runtime`.
- Moving persisted policy/layout fields out of `Graph`.
- Shrinking parent groups after child movement.
- Nested parent cascading unless the first implementation evidence proves it is required for a
  correct v1 contract.

## Starting Assumptions

| Assumption | Confidence | Evidence | Consequence if wrong |
| --- | --- | --- | --- |
| Parent expansion belongs in `runtime::drag`, not adapters. | High | Drag already plans movement, snap, extents, and store transactions. | Adapter crates would duplicate behavior and conformance would miss it. |
| Existing `GraphOp::SetGroupRect` is the right operation for parent expansion. | High | Core transactions already support group rect edits and undo/redo. | Add a new op only if rect expansion cannot express the behavior. |
| `expand_parent = false` must keep current parent clamp behavior. | High | XyFlow `calculateNodePosition` clamps `extent: parent` when expandParent is false. | Regression tests must protect current clamping. |
| Parent-left/top expansion needs a Jellyflow-specific coordinate decision. | High | XyFlow uses parent-relative child positions, while Jellyflow stores node positions in canvas space. | Document and test whether sibling compensation is required. |
| Nested parent cascading can wait for evidence. | Medium | Current Jellyflow groups are rect resources rather than nested node containers. | Split a follow-on if adapter tests expose nested expansion gaps. |

## Architecture Direction

Deepen `runtime::drag` instead of pushing expansion into adapters:

```text
canvas-space drag target
  -> drag candidates and constraints
  -> parent expansion planning
  -> GraphTransaction(SetNodePos..., SetGroupRect..., sibling SetNodePos...)
  -> NodeGraphStore trace and conformance fixtures
```

The public request should remain small if possible. The planner already has enough graph, policy,
node size, origin, parent, and extent context to decide expansion. Any new public plan output should
describe what happened, not require adapters to participate in the calculation.

## Coordinate Decision

XyFlow compensates non-dragged siblings when a parent expands left or upward because child node
positions are parent-relative: changing the parent position would otherwise move siblings visually.
Jellyflow stores node positions in canvas space and group rects as independent canvas-space
resources. Expanding a group rect left or upward does not move child nodes by itself, so the runtime
does not add sibling `SetNodePos` compensation ops for v1. If a future adapter introduces
parent-relative rendering, that adapter must either translate into Jellyflow's canvas-space model or
open a new ADR-backed coordinate-model lane.

## Source Coverage

| Source | State | Evidence | Impact |
| --- | --- | --- | --- |
| User goal | COVERED | Continue XyFlow-feel headless architecture work | Makes parent expansion the next node-drag parity lane. |
| `CONTEXT.md` | COVERED | Lists parent expansion/group resizing as a likely follow-on | Confirms this is runtime behavior, not renderer work. |
| ADR 0001 | COVERED | Headless engine boundary | Keeps Fret and renderer details outside this lane. |
| ADR 0002 | COVERED | Persisted model/policy fields stay in `Graph` for v1 | Reuse existing `Node.expand_parent` and `Node.extent`. |
| ADR 0003 | COVERED | Layered headless testing before renderer smoke | Guides runtime/conformance/template coverage. |
| XyFlow store update path | COVERED | `updateNodePositions` calls `handleExpandParent` after position changes | Confirms drag updates trigger parent expansion. |
| XyFlow parent expansion helper | COVERED | `handleExpandParent` expands parent rect and compensates siblings | Defines the subtle left/top behavior to evaluate. |
| Current Jellyflow drag planner | COVERED | Emits only `SetNodePos` operations | Identifies the exact shallow seam to deepen. |
| Renderer adapter crates | OUT_OF_SCOPE | Future adapter workstreams | Renderer smoke remains outside runtime. |

## Refactor Brief

- **Intent**: remove the accidental shallow meaning of `expand_parent = true` as only "do not clamp"
  and make it a transaction-level parent expansion behavior.
- **Scope**: `jellyflow-runtime::runtime::drag`, focused runtime drag tests, conformance/template
  surfaces if adapter traces need new fixture vocabulary, and closeout docs.
- **Deletion plan**: replace the current `NodeExtent::Parent => None` expansion shortcut with a
  tested expansion planner. Do not delete persisted compatibility fields.
- **Boundary plan**: keep raw input and renderer behavior outside runtime; keep graph mutation in
  normal `GraphTransaction` operations; keep public drag request shape minimal.
- **Testing plan**: start with focused runtime tests, then add conformance/template traces only once
  transaction semantics are stable.
- **Risk plan**: protect existing clamping for `expand_parent = false`; keep op ordering
  deterministic; split nested or resize-specific parity if it would broaden the lane.
- **Workflow plan**: durable workstream with vertical tasks and autonomous commits after each
  verified slice.
- **Scale plan**: medium architecture/refactor lane, not a one-off patch.

## Closeout Condition

This lane can close when:

- runtime drag planning implements or explicitly splits parent expansion semantics;
- false-policy clamping and true-policy expansion are covered by focused tests;
- multi-node and sibling behavior is deterministic and documented;
- conformance/template coverage proves adapter-facing traces if needed;
- README/runtime README explain the parent expansion boundary;
- fresh package, clippy, JSON, and diff gates pass;
- renderer, resize, nested-cascade, or platform-specific follow-ons are split or deferred.

## Closeout Summary

Closed on 2026-06-02. `runtime::drag` now plans parent expansion through deterministic
`SetGroupRect` ops, preserves `expand_parent = false` parent extent clamping, proves multi-parent
ordering and left/top canvas-space sibling behavior, and exposes adapter-facing traces through the
existing `ApplyNodeDrag` conformance action and the headless adapter template. Resize handles,
nested parent cascading, parent-relative coordinate semantics, and renderer smoke remain follow-ons
outside this runtime lane.
