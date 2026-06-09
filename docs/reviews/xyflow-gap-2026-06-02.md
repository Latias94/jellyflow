# XyFlow Gap Review - 2026-06-02

## Executive Summary

Jellyflow is already close to XyFlow at the intended headless boundary. The
important graph, store, policy, drag, connection, delete, viewport, geometry,
projection, callback, conformance, and adapter-template surfaces exist in
`jellyflow-core` or `jellyflow-runtime`.

The remaining gap is not "build XyFlow again". Since the original review,
several headless seams have been closed: pointer resize sessions and lifecycle
callbacks, constrained viewport pan/zoom entrypoints, a controlled graph facade,
aggregate rendering-query results, and visible edge culling. The remaining work
is now mostly optional precision/performance work and adapter-owned browser/UI
inventories.
React UI components, DOM measurement, providers, wrappers, minimap, background,
controls, portals, and accessibility text should stay out of the headless crates
and belong to future adapter crates.

This document is now a refreshed backlog snapshot after the runtime-deepening
work; the document itself does not imply additional behavior changes.

## Boundary

The review follows ADR 0001 and ADR 0003:

- `jellyflow-core` owns the portable graph document, stable IDs, type
  descriptors, and interaction policy value types
  (`docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md:28`).
- `jellyflow-runtime` owns headless I/O, view state, rules, schema/profile,
  store/apply/callback, and controlled-mode substrate
  (`docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md:32`).
- Renderer, DOM, platform, `wgpu`, `winit`, Fret UI, portals, overlays, and
  screenshot/pixel smoke tests belong outside the headless crates
  (`docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md:39`,
  `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md:42`).
- XyFlow feel should be tested through behavior contracts first:
  runtime contracts, `runtime::xyflow` projections/callbacks, adapter
  conformance scenarios, and external smoke
  (`docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md:31`).

## Source Map

XyFlow source areas reviewed:

- `repo-ref/xyflow/packages/system/src/xydrag/XYDrag.ts`: drag sessions,
  snap, extents, multi-drag, auto-pan, drag lifecycle.
- `repo-ref/xyflow/packages/system/src/xyhandle/XYHandle.ts`: connection
  start/end, closest target handle, DOM hit priority, strict/loose validation,
  connection radius, auto-pan.
- `repo-ref/xyflow/packages/system/src/xypanzoom/XYPanZoom.ts`: d3 pan/zoom,
  `scaleExtent`, `translateExtent`, constrained viewport setters.
- `repo-ref/xyflow/packages/system/src/xyresizer/XYResizer.ts`: pointer resize
  sessions, keep-aspect-ratio, boundary/child extent corrections.
- `repo-ref/xyflow/packages/system/src/utils/graph.ts`: `getNodesInside`,
  `fitViewport`, `calculateNodePosition`, `getElementsToRemove`.
- `repo-ref/xyflow/packages/system/src/utils/store.ts`: DOM measurement,
  `updateNodeInternals`, parent expansion.
- `repo-ref/xyflow/packages/system/src/utils/edges/*`: edge visibility,
  z-index elevation, reconnect helpers, path math.
- `repo-ref/xyflow/packages/react/src/utils/changes.ts`: React
  `applyNodeChanges`/`applyEdgeChanges` behavior.
- `repo-ref/xyflow/packages/react/src/hooks` and `components`: delete,
  fitView, visible ids, keyboard movement, node focus auto-pan, wrappers,
  minimap, controls, background, toolbar, providers.

Jellyflow source areas reviewed:

- `crates/jellyflow-core/src/core/model/*`: graph/node/edge model.
- `crates/jellyflow-core/src/ops/*`: transactions, mutation, diff, normalize,
  history, fragments.
- `crates/jellyflow-runtime/src/io/config/interaction/config.rs`: resolved
  XyFlow-shaped interaction configuration.
- `crates/jellyflow-runtime/src/runtime/{drag,resize,connection,delete,keyboard,selection,viewport,auto_pan,geometry,rendering,xyflow,conformance}`.
- `templates/headless-adapter/src/lib.rs`: copyable adapter conformance smoke
  surface.

## Coverage Matrix

| Area | Status | XyFlow Evidence | Jellyflow Evidence | Gap |
| --- | --- | --- | --- | --- |
| Graph model and store commit semantics | covered | React store defaults and system utils model interaction state (`packages/react/src/store/initialState.ts:90`, `packages/system/src/utils/store.ts:320`) | `Graph` uses stable maps for nodes/ports/edges/groups (`crates/jellyflow-core/src/core/model/graph.rs:16`); `GraphTransaction` is reversible and atomic (`crates/jellyflow-core/src/ops/transaction/batch.rs:7`) | Different data structures are intentional. Do not copy React array/store internals. |
| Node/edge change projection and controlled apply helpers | covered | React `applyChanges` handles add/remove/replace/select/position/dimensions with array-order semantics (`packages/react/src/utils/changes.ts:1`) | `runtime::xyflow` maps transactions to node/edge changes and back (`crates/jellyflow-runtime/src/runtime/xyflow/changes/mod.rs:1`); best-effort graph apply helpers and `ControlledGraph` exist (`crates/jellyflow-runtime/src/runtime/xyflow/apply/mod.rs:1`, `crates/jellyflow-runtime/src/runtime/xyflow/controlled.rs:1`); ordered adapter-array helpers cover exact React-style apply ordering and UI fields (`crates/jellyflow-runtime/src/runtime/xyflow/apply/ordered.rs:1`) | Graph-backed helpers intentionally remain document-model helpers; React array UI fields are represented only in adapter-owned ordered elements. |
| Connection/reconnection rules | partial | `XYHandle` resolves closest handles, strict/loose mode, `isValidConnection`, connectability, and connection radius (`packages/system/src/xyhandle/XYHandle.ts:20`, `packages/system/src/xyhandle/XYHandle.ts:256`) | Runtime has strict/loose target semantics (`crates/jellyflow-runtime/src/runtime/connection/target.rs:167`), closest handle math (`crates/jellyflow-runtime/src/runtime/connection/handles.rs:89`), handle-candidate target resolution (`crates/jellyflow-runtime/src/runtime/connection/target.rs:202`), connect/reconnect planners (`crates/jellyflow-runtime/src/rules/connection/connect.rs:11`, `crates/jellyflow-runtime/src/rules/connection/reconnect/planner.rs:14`) | DOM handle-under-pointer priority remains adapter-owned; headless candidate resolution is covered. |
| Delete and keyboard deletion | covered | `getElementsToRemove` cascades nodes/edges and awaits `onBeforeDelete` (`packages/system/src/utils/graph.ts:420`); React `deleteElements` uses the async result (`packages/react/src/hooks/useReactFlow.ts:150`) | Delete planner, key gate, cascaded edge removal, policy rejection, selection cleanup, and pre-delete `Accept`/`Veto`/`Replace` resolution contracts exist (`crates/jellyflow-runtime/src/runtime/delete/planner.rs:10`, `crates/jellyflow-runtime/src/runtime/delete/types.rs:7`, `crates/jellyflow-runtime/src/runtime/delete/store.rs:24`, `crates/jellyflow-runtime/src/runtime/tests/delete.rs:54`) | Async hook execution, confirmation UI, and platform scheduling remain adapter-owned. |
| Selection box and selection callbacks | covered | Pane selection uses screen-space selection rect and `getNodesInside`; edges can be selected through selected nodes (`packages/react/src/container/Pane/index.tsx:190`) | Runtime computes canvas-space selection with full/partial inclusion, selectable policy, edge selection, additive selection (`crates/jellyflow-runtime/src/runtime/selection/compute.rs:13`) | Screen overlay geometry and pointer event ownership are adapter-owned. |
| Node drag, snapping, extents, parent expansion, keyboard nudge | covered | `XYDrag` handles multi-drag, snap offsets, node extent adjustment, auto-pan, drag lifecycle, and delete-while-drag abort (`packages/system/src/xydrag/XYDrag.ts:130`, `packages/system/src/xydrag/XYDrag.ts:232`) | Runtime has screen threshold, gesture claim, selected-node candidates, snap, adjusted global extents, parent expansion, nudge, nested parent canvas-space conformance, and delete-during-drag lifecycle fixtures (`crates/jellyflow-runtime/src/runtime/drag/activation.rs:3`, `crates/jellyflow-runtime/src/runtime/drag/planner.rs:14`, `crates/jellyflow-runtime/src/runtime/drag/constraints/items.rs:9`, `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/fixture_runner/node_drag.rs:111`) | XyFlow node-owned relative coordinates remain intentionally outside Jellyflow's group-as-frame model. |
| Node resize | partial | `XYResizer` owns pointer resize sessions, keep-aspect-ratio, boundaries, child extent correction, parent expansion clamps (`packages/system/src/xyresizer/XYResizer.ts:100`, `packages/system/src/xyresizer/XYResizer.ts:190`) | Runtime supports target-size resize, pointer resize sessions, keep-aspect-ratio, min/max constraints, direction-driven position changes, node origin, parent group expansion, store commit, lifecycle callbacks, and conformance fixtures (`crates/jellyflow-runtime/src/runtime/resize/session.rs:1`, `crates/jellyflow-runtime/src/runtime/resize/planner.rs:10`, `crates/jellyflow-runtime/src/runtime/resize/parent_expansion.rs:1`) | Remaining parity gap: XyFlow's node-owned child extent correction and parent-relative coordinate behavior are not modeled by Jellyflow's group-as-frame contract. |
| Viewport pan/zoom, scroll, double-click, animation, inertia, fit view | partial | `XYPanZoom` constrains viewport with d3 `scaleExtent` and `translateExtent`, supports pan/scroll/zoom policies and setters (`packages/system/src/xypanzoom/XYPanZoom.ts:40`, `packages/system/src/xypanzoom/XYPanZoom.ts:204`) | Runtime has pan/zoom transforms, constrained pan/zoom entrypoints, scroll policy, double-click animation, animation frames, inertia, fit-view math, and tests (`crates/jellyflow-runtime/src/runtime/viewport/transform.rs:6`, `crates/jellyflow-runtime/src/runtime/store/view/state.rs:52`, `crates/jellyflow-runtime/src/runtime/fit_view/compute.rs:9`) | Low-level unconstrained setters intentionally remain; exact d3 edge-case parity can be added only if adapters need it. |
| Auto-pan | covered | XyFlow auto-pans during node drag/connect and node focus; selection drag also has screen-rect auto-pan paths (`packages/system/src/xydrag/XYDrag.ts:232`, `packages/system/src/xyhandle/XYHandle.ts:20`, `packages/react/src/components/NodeWrapper/index.tsx:160`) | Runtime has a deterministic auto-pan kernel, activation gates for node drag/connect/node focus, selection auto-pan request/store helpers, and conformance fixtures (`crates/jellyflow-runtime/src/runtime/auto_pan/types.rs:58`, `crates/jellyflow-runtime/src/runtime/auto_pan/planner.rs:45`, `crates/jellyflow-runtime/src/runtime/auto_pan/store.rs:23`) | Raw selection rectangle tracking, pointer/session ownership, and frame scheduling remain adapter-owned. |
| Edge path geometry and hit testing | covered | XyFlow system has bezier path, label, and reconnect helpers (`packages/system/src/utils/edges/bezier-edge.ts:1`) | Runtime has bezier path generation, label math, straight/smooth-step helpers, and numeric hit testing with interaction width (`crates/jellyflow-runtime/src/runtime/geometry/paths/bezier.rs:19`, `crates/jellyflow-runtime/src/runtime/geometry/hit_test.rs:40`) | Exact SVG path string formatting is adapter-owned unless a renderer needs it. |
| Visible nodes and node render order | covered | React visible node hook uses `getNodesInside` (`packages/react/src/hooks/useVisibleNodeIds.ts:1`) | Runtime resolves visible node ids, visible node render order, and aggregate rendering query results with culling, hidden policy, node origin, fallback size, and elevation (`crates/jellyflow-runtime/src/runtime/rendering/visibility.rs:11`, `crates/jellyflow-runtime/src/runtime/rendering/query.rs:1`) | None at the current headless contract level. |
| Visible edge culling | covered | React visible edge hook calls `isEdgeVisible` using source/target node bounds (`packages/react/src/hooks/useVisibleEdgeIds.ts:1`); system utility has edge visibility math (`packages/system/src/utils/edges/general.ts:69`) | Runtime resolves visible edge ids and visible edge render order using endpoint node bounds, hidden policy, node origin, fallback size, and edge elevation (`crates/jellyflow-runtime/src/runtime/rendering/visibility.rs:136`, `crates/jellyflow-runtime/src/runtime/rendering/visibility.rs:198`) | Path-geometry clipping remains renderer-owned unless a future adapter needs it. |
| Render order and selected elevation | covered | XyFlow elevates selected nodes/edges through z-index helpers (`packages/system/src/utils/edges/general.ts:1`) | Runtime resolves node/group/edge paint order and selected-edge elevation through connected selected nodes (`crates/jellyflow-runtime/src/runtime/rendering/order.rs:133`, `crates/jellyflow-runtime/src/runtime/rendering/order.rs:182`) | DOM z-index class implementation is adapter-owned. |
| Conformance harness and adapter template | covered | XyFlow behavior is source-backed, but XyFlow itself does not provide Jellyflow fixtures | Jellyflow has conformance actions for drag/resize sessions/connect/reconnect/delete/auto-pan/constrained viewport/visible node and edge rendering/callback traces, plus state assertions such as node position (`crates/jellyflow-runtime/src/runtime/conformance/scenario/action.rs:44`, `crates/jellyflow-runtime/src/runtime/conformance/runner/actions/graph.rs:33`, `templates/headless-adapter/src/lib.rs:33`) | Future adapter crates can add renderer-specific smoke fixtures, but the headless harness has the needed runtime vocabulary. |
| React/DOM UI inventory | adapter-owned | React package owns wrappers, store binding, hooks, components, minimap, controls, background, toolbar, portals, DOM node internals (`packages/react/src/store/index.ts:160`, `packages/react/src/components`) | ADRs require headless crates to stay renderer/platform-free | Future adapter crates should own this inventory. |
| Browser/provider/SSR/accessibility text | adapter-owned | React provider, aria descriptions, DOM focus/measurement, ResizeObserver, viewport DOM transform are React/package concerns | No equivalent should exist in core/runtime | Do not implement inside `jellyflow-core` or `jellyflow-runtime`. |
| XyFlow browser screenshots/pixel parity | intentionally out of scope | XyFlow validates browser/DOM behavior in its own stack | ADR 0003 puts screenshots/pixel smoke in adapter crates (`docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md:42`) | Out of scope for this task and for headless crates. |

## Detailed Findings

### 1. Jellyflow's Graph Boundary Is Stronger Than XyFlow's React Store Boundary

XyFlow's React package is optimized around arrays, Zustand store state, DOM
measurement, and callback projection. Jellyflow instead has a typed graph
document and reversible transactions:

- `Graph` stores nodes, ports, edges, groups, sticky notes, symbols, and imports
  in stable maps (`crates/jellyflow-core/src/core/model/graph.rs:16`).
- Nodes and edges already carry XyFlow-shaped optional interaction overrides:
  selectable, focusable, draggable/connectable, deletable, hidden, parent,
  extent, expand parent, interaction width, and reconnectable
  (`crates/jellyflow-core/src/core/model/node.rs:28`,
  `crates/jellyflow-core/src/core/model/edge.rs:45`).
- Transactions carry reversible ops and are the undo/redo boundary
  (`crates/jellyflow-core/src/ops/transaction/batch.rs:7`,
  `crates/jellyflow-core/src/ops/transaction/op.rs:13`).

Recommendation: keep this divergence. Parity work should compare behavior, not
data structure shape.

### 2. Controlled-Mode Compatibility Covers React `applyChanges` Ordering

XyFlow's `applyChanges` supports array-focused behavior such as add index,
remove/replace precedence, shallow item copying, dimensions updates, and
`resizing` flags (`repo-ref/xyflow/packages/react/src/utils/changes.ts:1`).

Jellyflow has:

- `NodeChange`/`EdgeChange` models (`crates/jellyflow-runtime/src/runtime/xyflow/changes/node.rs:8`,
  `crates/jellyflow-runtime/src/runtime/xyflow/changes/edge.rs:5`);
- transaction-to-change projection (`crates/jellyflow-runtime/src/runtime/xyflow/projection/mod.rs:1`);
- change-to-transaction conversion (`crates/jellyflow-runtime/src/runtime/xyflow/transaction/mod.rs:1`);
- `ControlledGraph` as an adapter-friendly controlled-state facade
  (`crates/jellyflow-runtime/src/runtime/xyflow/controlled.rs:1`);
- controlled apply helpers that intentionally "apply what exists, ignore what
  does not" for graph-backed state (`crates/jellyflow-runtime/src/runtime/xyflow/apply/mod.rs:1`);
- ordered adapter-array helpers for exact React-style add/remove/replace,
  add-index insertion, `select`, `position.dragging`, `dimensions.measured`,
  `dimensions.setAttributes`, and `resizing` behavior
  (`crates/jellyflow-runtime/src/runtime/xyflow/apply/ordered.rs:1`).

Recommendation: use graph-backed helpers when synchronizing Jellyflow documents.
Use the ordered `XyFlowNodeElement`/`XyFlowEdgeElement` helpers when an adapter
needs React Flow controlled-array parity, including UI-only fields that do not
belong in `jellyflow-core::Node`.

### 3. Connection Runtime Has Candidate Target Contracts

XyFlow's `XYHandle` combines several layers: pointer capture, DOM lookup,
closest-handle search, handle-under-pointer priority, connectability classes,
strict/loose mode, `isValidConnection`, connection radius, callbacks, and
auto-pan.

Jellyflow already owns the headless parts:

- connection drag threshold in screen coordinates
  (`crates/jellyflow-runtime/src/runtime/connection/activation.rs:3`);
- closest handle geometry and XyFlow tie semantics
  (`crates/jellyflow-runtime/src/runtime/connection/handles.rs:89`);
- strict/loose target validation and feedback enum
  (`crates/jellyflow-runtime/src/runtime/connection/target.rs:167`);
- adapter-provided handle-candidate target resolution
  (`crates/jellyflow-runtime/src/runtime/connection/target.rs:202`);
- connect/reconnect planners with duplicate/capacity/policy checks
  (`crates/jellyflow-runtime/src/rules/connection/connect.rs:11`,
  `crates/jellyflow-runtime/src/rules/connection/reconnect/planner.rs:14`);
- conformance traces for connect/reconnect projections
  (`crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/projections.rs:25`).

Gap: XyFlow's "handle below cursor wins over closest handle" is a DOM hit-test
priority, so it should not live in core. Future adapter crates should prove that
their rendered handle inventory maps into `ConnectionTargetCandidate` values
before calling the headless resolver.

### 4. Delete Covers Pre-Delete Veto And Substitute Planning

XyFlow can ask `onBeforeDelete` to reject or replace the deletion set before
applying deletions. Jellyflow keeps the actual await/UI scheduling in adapters,
but now exposes the headless request and resolution contract:

- atomic delete plans and cascaded consistency edits
  (`crates/jellyflow-runtime/src/rules/plans.rs:113`);
- selected node/edge deletion and key-bound deletion
  (`crates/jellyflow-runtime/src/runtime/delete/planner.rs:10`);
- selection cleanup and policy rejection tests
  (`crates/jellyflow-runtime/src/runtime/tests/delete.rs:9`);
- pre-delete `DeleteElements`, `PreDeleteRequest`, and `PreDeleteResolution`
  types for adapter-owned async hooks (`crates/jellyflow-runtime/src/runtime/delete/types.rs:7`);
- store-level `prepare_delete_selection`, `prepare_delete_selection_for_key`,
  and `apply_pre_delete_resolution` entrypoints
  (`crates/jellyflow-runtime/src/runtime/delete/store.rs:24`);
- keyboard intent routing
  (`crates/jellyflow-runtime/src/runtime/keyboard/store.rs:8`).

Recommendation: adapters should call a prepare method, await their own
`onBeforeDelete` hook or confirmation UI, then pass `Accept`, `Veto`, or
`Replace` back to the store. The store revalidates replacement sets through
normal delete policy before committing.

### 5. Drag Semantics Are Broadly Covered; Nested Parent and Lifecycle Need Fixtures

Jellyflow matches a lot of `XYDrag`:

- screen-space drag activation threshold
  (`crates/jellyflow-runtime/src/runtime/drag/activation.rs:3`);
- pointer gesture ownership between selection, connection, and node drag
  (`crates/jellyflow-runtime/src/runtime/drag/pointer_gesture.rs:9`);
- selected-node co-drag candidates and policy filtering
  (`crates/jellyflow-runtime/src/runtime/drag/candidates.rs:20`);
- JS-style snap rounding (`crates/jellyflow-runtime/src/runtime/drag/constraints/snap.rs:6`);
- adjusted global extent for multi-drag
  (`crates/jellyflow-runtime/src/runtime/drag/constraints/items.rs:41`);
- parent expansion ops (`crates/jellyflow-runtime/src/runtime/drag/parent_expansion.rs:11`);
- keyboard nudge via the same move planner
  (`crates/jellyflow-runtime/src/runtime/drag/planner.rs:52`).

Jellyflow's planner is canvas-space and deterministic. Runtime-deepening now
locks the important adapter contract points with conformance fixtures:

- nested parent drag keeps node positions in Jellyflow canvas space while using
  groups as frames (`crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/fixture_runner/node_drag.rs:111`);
- deletion during an active drag is represented as a normal delete commit
  followed by an adapter-emitted canceled drag end
  (`crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/fixture_runner/node_drag.rs:166`);
- `AssertNodePosition` lets fixtures prove state-level coordinates, not only
  store event kinds (`crates/jellyflow-runtime/src/runtime/conformance/scenario/action.rs:44`).

Gap: exact XyFlow node-owned relative position math is intentionally not copied
into the runtime while Jellyflow's model stores node positions in canvas space.

### 6. Resize Session and Group Containment Seams Exist

XyFlow's `XYResizer` is a full pointer-session engine. It computes next
dimensions from pointer deltas, supports `keepAspectRatio`, clamps to min/max,
parent and child extents, expands parents, adjusts children on top/left
resizes, and emits lifecycle callbacks.

Jellyflow now has the headless session contract adapters need:

- target-size requests with min/max constraints
  (`crates/jellyflow-runtime/src/runtime/resize/types.rs:120`);
- pointer-derived resize requests and `NodeResizeSession`
  (`crates/jellyflow-runtime/src/runtime/resize/session.rs:1`);
- keep-aspect-ratio solving in the resize planner
  (`crates/jellyflow-runtime/src/runtime/resize/planner.rs:134`);
- store methods that emit start/update/end gesture events around the commit
  (`crates/jellyflow-runtime/src/runtime/resize/store.rs:61`);
- direction-driven position updates and node-origin support
  (`crates/jellyflow-runtime/src/runtime/resize/planner.rs:84`);
- group-based parent expansion for children with `expand_parent = true`
  (`crates/jellyflow-runtime/src/runtime/resize/parent_expansion.rs:1`);
- tests for constraints, top/left position shifts, origin fallback/override,
  and invalid/noop requests (`crates/jellyflow-runtime/src/runtime/tests/resize.rs:73`);
- adapter-template resize-session smoke (`templates/headless-adapter/src/lib.rs:277`).

Gap: XyFlow still has node-owned child extent correction and parent-relative
child adjustment behavior during top/left resizes. Jellyflow intentionally stores
nodes in canvas space and treats groups as frames, so this should remain out of
runtime until a future model decision introduces node-owned nested coordinates.

### 7. Viewport `translateExtent` Has Constrained Entrypoints

Jellyflow already has renderer-neutral viewport math:

- pan and anchored zoom transforms (`crates/jellyflow-runtime/src/runtime/viewport/transform.rs:6`);
- optional `ViewportConstraints` and `constrain_viewport`
  (`crates/jellyflow-runtime/src/runtime/viewport/transform.rs:63`);
- store-level constrained pan and zoom entrypoints that use configured
  `translate_extent` when the adapter provides viewport size
  (`crates/jellyflow-runtime/src/runtime/store/view/state.rs:52`);
- scroll/pinch/wheel priority similar to XyFlow
  (`crates/jellyflow-runtime/src/runtime/viewport/gesture/scroll.rs:13`);
- double-click zoom animation planning
  (`crates/jellyflow-runtime/src/runtime/viewport/gesture/double_click.rs:9`);
- deterministic animation and inertia frames
  (`crates/jellyflow-runtime/src/runtime/viewport/animation.rs:66`,
  `crates/jellyflow-runtime/src/runtime/viewport/inertia.rs:32`);
- fit-view helpers with node origin and zoom clamps
  (`crates/jellyflow-runtime/src/runtime/fit_view/compute.rs:9`).

Gap: exact d3 edge-case parity is not fully characterized. The low-level
`apply_viewport_pan` and `apply_viewport_zoom` methods intentionally remain
unconstrained for callers that already solved viewport bounds externally; an
adapter that wants XyFlow-style bounds should call the constrained entrypoints.

Suggested task: `Viewport d3 parity edge-case conformance`.

### 8. Auto-Pan Kernel And Selection Contract Exist

Jellyflow's auto-pan is deterministic and reusable:

- `AutoPanActivation` supports node drag, connect, node focus, and `Always` for
  adapter-owned flows (`crates/jellyflow-runtime/src/runtime/auto_pan/types.rs:7`);
- `compute_auto_pan` derives per-frame screen delta from pointer proximity
  (`crates/jellyflow-runtime/src/runtime/auto_pan/planner.rs:6`);
- `SelectionAutoPanRequest` and `compute_selection_auto_pan` name the selection
  workflow while reusing the shared edge-proximity math
  (`crates/jellyflow-runtime/src/runtime/auto_pan/types.rs:58`,
  `crates/jellyflow-runtime/src/runtime/auto_pan/planner.rs:45`);
- store application publishes normal viewport changes
  (`crates/jellyflow-runtime/src/runtime/auto_pan/store.rs:13`), including
  selection auto-pan via `NodeGraphStore::apply_selection_auto_pan`
  (`crates/jellyflow-runtime/src/runtime/auto_pan/store.rs:23`).

Gap: XyFlow selection auto-pan depends on a screen-space selection rectangle and
pointer/session ownership. Those remain adapter responsibilities; the shared
runtime contract now covers the viewport-motion frame once the adapter supplies
the pointer position.

### 9. Rendering Helpers Cover Node And Edge Visibility

Jellyflow covers renderer-facing ordering and visibility:

- visible node IDs with viewport culling and hidden policy
  (`crates/jellyflow-runtime/src/runtime/rendering/visibility.rs:11`);
- visible node render order by composing culling and order
  (`crates/jellyflow-runtime/src/runtime/rendering/visibility.rs:82`);
- visible edge IDs with XyFlow-style endpoint node-box union culling
  (`crates/jellyflow-runtime/src/runtime/rendering/visibility.rs:136`);
- visible edge render order by composing edge culling and edge order
  (`crates/jellyflow-runtime/src/runtime/rendering/visibility.rs:198`);
- aggregate renderer-facing order and visibility result
  (`crates/jellyflow-runtime/src/runtime/rendering/query.rs:50`);
- node/group/edge order with selected elevation
  (`crates/jellyflow-runtime/src/runtime/rendering/order.rs:133`,
  `crates/jellyflow-runtime/src/runtime/rendering/order.rs:182`);
- template smoke for visible node and edge rendering
  (`templates/headless-adapter/src/lib.rs:550`,
  `templates/headless-adapter/src/lib.rs:662`).

Gap: path-level edge clipping is intentionally not part of the headless
contract. XyFlow's reusable helper uses endpoint node bounds, so Jellyflow
matches that seam and leaves renderer-specific path clipping to adapters.

### 10. Geometry And Hit Testing Are In Good Shape

Jellyflow has renderer-neutral edge path and hit-test primitives:

- bezier path and label math (`crates/jellyflow-runtime/src/runtime/geometry/paths/bezier.rs:19`);
- numeric edge path distance/hit testing with configurable interaction width
  (`crates/jellyflow-runtime/src/runtime/geometry/hit_test.rs:40`);
- geometry adapter conformance tests
  (`crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/geometry.rs`).

Gap: real spatial indexing is not implemented yet. `NodeGraphSpatialIndexTuning`
is currently a reserved tuning surface, and visible-node culling still uses a
linear lookup scan (`crates/jellyflow-runtime/src/runtime/rendering/visibility.rs:53`).
This should wait for adapter workloads.

Suggested task: `Spatial index backend for rendering queries`.

## Top Follow-Up Tasks

Recently closed by runtime-deepening work:

- Pointer resize session seam, including start/update/end gesture callbacks.
- Viewport `translateExtent` support through constrained pan/zoom entrypoints.
- Controlled graph facade for adapter-owned graph state.
- Aggregate rendering query result for renderer-facing order and visibility.
- Visible edge culling and visible edge render-order contracts.
- Resize parent group expansion with conformance traces.
- Connection target resolution from adapter-provided handle candidates.
- Selection auto-pan request/store/conformance contract.
- XyFlow ordered adapter-array apply helpers for exact `applyChanges` parity.
- Async pre-delete veto and substitute delete planning contract.
- Nested parent drag and delete-during-drag lifecycle conformance.

| Priority | Suggested Trellis Task | Owner Area | Why |
| --- | --- | --- | --- |
| P3 | Viewport d3 parity edge-case conformance | runtime viewport/conformance | Only needed if an adapter depends on exact d3 constraint edge cases. |
| P3 | Spatial index backend for rendering queries | runtime rendering/lookups | Optimize only after real workloads show linear scans are insufficient. |
| P3 | React-style adapter UI inventory | future adapter crates | Minimap, controls, background, toolbar, provider, DOM measurement, and accessibility belong outside headless crates. |

## Adapter-Owned Inventory

Do not implement these in `jellyflow-core` or `jellyflow-runtime`:

- React providers, hooks, Zustand store binding, wrappers, portals, SSR shape.
- DOM node internals, ResizeObserver, handle DOM querying, class names, aria
  text, keyboard focus DOM behavior.
- Minimap, controls, background, panels, toolbar, node resizer UI widgets,
  connection line rendering.
- Browser screenshots, pixel checks, wgpu/egui/Fret renderer smoke.

Future adapters should consume the public runtime contracts and add their own
renderer-specific conformance/smoke layers.

## Bottom Line

Jellyflow is not far from XyFlow's reusable `packages/system` behavior at the
headless contract level. The architecture is already pointed in the right
direction: core model and transactions, runtime policy/planners, XyFlow-style
projection/callbacks, conformance fixtures, and adapter template all exist.

The next work should stay narrow and contract-driven, not become a rewrite:
viewport d3 edge-case fixtures if an adapter needs them, spatial indexing after
real workload evidence, and future adapter UI inventories.
