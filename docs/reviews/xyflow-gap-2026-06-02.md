# XyFlow Gap Review - 2026-06-02

## Executive Summary

Jellyflow is already close to XyFlow at the intended headless boundary. The
important graph, store, policy, drag, connection, delete, viewport, geometry,
projection, callback, conformance, and adapter-template surfaces exist in
`jellyflow-core` or `jellyflow-runtime`.

The remaining gap is not "build XyFlow again". It is mostly precision work:
exact pointer-resize session semantics, visible edge culling, viewport
`translateExtent` constraints, async pre-delete/veto behavior, selection
auto-pan scenarios, and a more explicit adapter-owned connection-handle target
pipeline. React UI components, DOM measurement, providers, wrappers, minimap,
background, controls, portals, and accessibility text should stay out of the
headless crates and belong to future adapter crates.

No runtime or core behavior changes were made for this review.

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
| Node/edge change projection and controlled apply helpers | partial | React `applyChanges` handles add/remove/replace/select/position/dimensions with array-order semantics (`packages/react/src/utils/changes.ts:1`) | `runtime::xyflow` maps transactions to node/edge changes and back (`crates/jellyflow-runtime/src/runtime/xyflow/changes/mod.rs:1`); best-effort apply helpers exist (`crates/jellyflow-runtime/src/runtime/xyflow/apply/mod.rs:1`) | Missing exact React array semantics: `replace`, add index insertion, `dimensions.setAttributes`, `resizing`, and full change ordering equivalence. |
| Connection/reconnection rules | partial | `XYHandle` resolves closest handles, strict/loose mode, `isValidConnection`, connectability, and connection radius (`packages/system/src/xyhandle/XYHandle.ts:20`, `packages/system/src/xyhandle/XYHandle.ts:256`) | Runtime has strict/loose target semantics (`crates/jellyflow-runtime/src/runtime/connection/target.rs:97`), closest handle math (`crates/jellyflow-runtime/src/runtime/connection/handles.rs:89`), connect/reconnect planners (`crates/jellyflow-runtime/src/rules/connection/connect.rs:11`, `crates/jellyflow-runtime/src/rules/connection/reconnect/planner.rs:14`) | DOM handle-under-pointer priority and adapter conversion from rendered handles into candidate lists are not first-class conformance flows yet. |
| Delete and keyboard deletion | partial | `getElementsToRemove` cascades nodes/edges and awaits `onBeforeDelete` (`packages/system/src/utils/graph.ts:420`); React `deleteElements` uses the async result (`packages/react/src/hooks/useReactFlow.ts:150`) | Synchronous delete planner, key gate, cascaded edge removal, policy rejection, and selection cleanup exist (`crates/jellyflow-runtime/src/runtime/delete/planner.rs:10`, `crates/jellyflow-runtime/src/runtime/tests/delete.rs:9`) | Async pre-delete/veto/substitute deletion sets are missing. |
| Selection box and selection callbacks | covered | Pane selection uses screen-space selection rect and `getNodesInside`; edges can be selected through selected nodes (`packages/react/src/container/Pane/index.tsx:190`) | Runtime computes canvas-space selection with full/partial inclusion, selectable policy, edge selection, additive selection (`crates/jellyflow-runtime/src/runtime/selection/compute.rs:13`) | Screen overlay geometry and pointer event ownership are adapter-owned. |
| Node drag, snapping, extents, parent expansion, keyboard nudge | partial | `XYDrag` handles multi-drag, snap offsets, node extent adjustment, auto-pan, drag lifecycle, and delete-while-drag abort (`packages/system/src/xydrag/XYDrag.ts:130`, `packages/system/src/xydrag/XYDrag.ts:232`) | Runtime has screen threshold, gesture claim, selected-node candidates, snap, adjusted global extents, parent expansion, and nudge (`crates/jellyflow-runtime/src/runtime/drag/activation.rs:3`, `crates/jellyflow-runtime/src/runtime/drag/planner.rs:14`, `crates/jellyflow-runtime/src/runtime/drag/constraints/items.rs:9`) | Needs conformance for nested parent-relative coordinates, deletion during active drag, and full lifecycle sequencing. |
| Node resize | partial | `XYResizer` owns pointer resize sessions, keep-aspect-ratio, boundaries, child extent correction, parent expansion clamps (`packages/system/src/xyresizer/XYResizer.ts:100`, `packages/system/src/xyresizer/XYResizer.ts:190`) | Runtime supports target-size resize, min/max constraints, direction-driven position changes, node origin, store commit, tests/template smoke (`crates/jellyflow-runtime/src/runtime/resize/planner.rs:10`, `crates/jellyflow-runtime/src/runtime/tests/resize.rs:13`, `templates/headless-adapter/src/lib.rs:189`) | Biggest runtime parity gap: no pointer-session delta solver, keep-aspect-ratio, parent/child clamp correction, or resize lifecycle callbacks. |
| Viewport pan/zoom, scroll, double-click, animation, inertia, fit view | partial | `XYPanZoom` constrains viewport with d3 `scaleExtent` and `translateExtent`, supports pan/scroll/zoom policies and setters (`packages/system/src/xypanzoom/XYPanZoom.ts:40`, `packages/system/src/xypanzoom/XYPanZoom.ts:204`) | Runtime has pan/zoom transforms, scroll policy, double-click animation, animation frames, inertia, fit-view math, and tests (`crates/jellyflow-runtime/src/runtime/viewport/transform.rs:6`, `crates/jellyflow-runtime/src/runtime/viewport/gesture/scroll.rs:13`, `crates/jellyflow-runtime/src/runtime/fit_view/compute.rs:9`) | `translate_extent` is persisted config but not enforced by `apply_viewport_pan/zoom`; no d3-level constrained viewport equivalent. |
| Auto-pan | partial | XyFlow auto-pans during node drag/connect and node focus; selection drag also has screen-rect auto-pan paths (`packages/system/src/xydrag/XYDrag.ts:232`, `packages/system/src/xyhandle/XYHandle.ts:20`, `packages/react/src/components/NodeWrapper/index.tsx:160`) | Runtime has a deterministic auto-pan kernel, activation gates for node drag/connect/node focus, and an `Always` mode for adapter-owned flows (`crates/jellyflow-runtime/src/runtime/auto_pan/types.rs:7`, `crates/jellyflow-runtime/src/runtime/auto_pan/planner.rs:6`) | Selection-specific auto-pan policy and fixtures are not locked yet. |
| Edge path geometry and hit testing | covered | XyFlow system has bezier path, label, and reconnect helpers (`packages/system/src/utils/edges/bezier-edge.ts:1`) | Runtime has bezier path generation, label math, straight/smooth-step helpers, and numeric hit testing with interaction width (`crates/jellyflow-runtime/src/runtime/geometry/paths/bezier.rs:19`, `crates/jellyflow-runtime/src/runtime/geometry/hit_test.rs:40`) | Exact SVG path string formatting is adapter-owned unless a renderer needs it. |
| Visible nodes and node render order | covered | React visible node hook uses `getNodesInside` (`packages/react/src/hooks/useVisibleNodeIds.ts:1`) | Runtime resolves visible node ids and visible node render order with culling, hidden policy, node origin, fallback size, and elevation (`crates/jellyflow-runtime/src/runtime/rendering/visibility.rs:11`, `crates/jellyflow-runtime/src/runtime/rendering/order.rs:133`) | None at the current headless contract level. |
| Visible edge culling | missing | React visible edge hook calls `isEdgeVisible` using source/target node bounds (`packages/react/src/hooks/useVisibleEdgeIds.ts:1`); system utility has edge visibility math (`packages/system/src/utils/edges/general.ts:190`) | `rg` found visible node culling but no `resolve_visible_edge_ids` equivalent in runtime rendering | Add runtime edge visibility once endpoint/path/AABB semantics are settled. |
| Render order and selected elevation | covered | XyFlow elevates selected nodes/edges through z-index helpers (`packages/system/src/utils/edges/general.ts:1`) | Runtime resolves node/group/edge paint order and selected-edge elevation through connected selected nodes (`crates/jellyflow-runtime/src/runtime/rendering/order.rs:133`, `crates/jellyflow-runtime/src/runtime/rendering/order.rs:182`) | DOM z-index class implementation is adapter-owned. |
| Conformance harness and adapter template | partial | XyFlow behavior is source-backed, but XyFlow itself does not provide Jellyflow fixtures | Jellyflow has conformance actions for drag/resize/connect/reconnect/delete/auto-pan/viewport/visible nodes/render order and callback traces (`crates/jellyflow-runtime/src/runtime/conformance/scenario/action.rs:23`, `templates/headless-adapter/src/lib.rs:31`) | Need more gap-specific scenarios: pointer-resize sessions, visible edges, translate extent, selection auto-pan, async pre-delete. |
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

### 2. Controlled-Mode Compatibility Exists But Is Not Exact React `applyChanges`

XyFlow's `applyChanges` supports array-focused behavior such as add index,
remove/replace precedence, shallow item copying, dimensions updates, and
`resizing` flags (`repo-ref/xyflow/packages/react/src/utils/changes.ts:1`).

Jellyflow has:

- `NodeChange`/`EdgeChange` models (`crates/jellyflow-runtime/src/runtime/xyflow/changes/node.rs:8`,
  `crates/jellyflow-runtime/src/runtime/xyflow/changes/edge.rs:5`);
- transaction-to-change projection (`crates/jellyflow-runtime/src/runtime/xyflow/projection/mod.rs:1`);
- change-to-transaction conversion (`crates/jellyflow-runtime/src/runtime/xyflow/transaction/mod.rs:1`);
- controlled apply helpers that intentionally "apply what exists, ignore what
  does not" (`crates/jellyflow-runtime/src/runtime/xyflow/apply/mod.rs:1`).

Gap: there is no precise React `applyChanges` equivalence table. This matters
for adapters that want to mirror React Flow controlled-state callbacks exactly.

Suggested task: `XyFlow applyChanges precision conformance`.

### 3. Connection Runtime Is Solid; Target Selection Needs Adapter Contracts

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
  (`crates/jellyflow-runtime/src/runtime/connection/target.rs:97`);
- connect/reconnect planners with duplicate/capacity/policy checks
  (`crates/jellyflow-runtime/src/rules/connection/connect.rs:11`,
  `crates/jellyflow-runtime/src/rules/connection/reconnect/planner.rs:14`);
- conformance traces for connect/reconnect projections
  (`crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/projections.rs:25`).

Gap: the adapter-owned pipeline from rendered/DOM handle inventory to
`ConnectionHandleCandidate` and `ConnectionTargetInput` is not yet locked in a
template scenario. XyFlow's "handle below cursor wins over closest handle" is a
DOM hit-test priority, so it should not live in core, but the future adapter
should prove it.

Suggested task: `Adapter connection target resolution fixtures`.

### 4. Delete Covers Synchronous Semantics But Not Async `onBeforeDelete`

XyFlow can ask `onBeforeDelete` to reject or replace the deletion set before
applying deletions. Jellyflow currently plans and commits synchronous delete
transactions:

- atomic delete plans and cascaded consistency edits
  (`crates/jellyflow-runtime/src/rules/plans.rs:113`);
- selected node/edge deletion and key-bound deletion
  (`crates/jellyflow-runtime/src/runtime/delete/planner.rs:10`);
- selection cleanup and policy rejection tests
  (`crates/jellyflow-runtime/src/runtime/tests/delete.rs:9`);
- keyboard intent routing
  (`crates/jellyflow-runtime/src/runtime/keyboard/store.rs:8`).

Gap: no async pre-delete hook, veto, or replacement delete set. This should be
a runtime/store design if the adapter needs it, because the hook must run
before the authoritative transaction.

Suggested task: `Async pre-delete veto and substitute delete planning`.

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

Gaps: XyFlow's drag implementation includes session callbacks, active-node
state, deletion while dragging, and parent-relative position math. Jellyflow's
planner is canvas-space and deterministic, but it needs fixtures for nested
parent semantics and abort/lifecycle parity.

Suggested task: `Nested parent drag and drag lifecycle conformance`.

### 6. Resize Is The Largest Runtime Semantics Gap

XyFlow's `XYResizer` is a full pointer-session engine. It computes next
dimensions from pointer deltas, supports `keepAspectRatio`, clamps to min/max,
parent and child extents, expands parents, adjusts children on top/left
resizes, and emits lifecycle callbacks.

Jellyflow currently has a useful but smaller contract:

- target-size requests with min/max constraints
  (`crates/jellyflow-runtime/src/runtime/resize/types.rs:120`);
- direction-driven position updates and node-origin support
  (`crates/jellyflow-runtime/src/runtime/resize/planner.rs:84`);
- tests for constraints, top/left position shifts, origin fallback/override,
  and invalid/noop requests (`crates/jellyflow-runtime/src/runtime/tests/resize.rs:73`);
- adapter-template resize smoke (`templates/headless-adapter/src/lib.rs:189`).

Gap: no pointer-session solver, no keep-aspect-ratio mode, no parent/child
extent correction, no resize start/update/end callback surface. This is the top
runtime parity gap if the goal is "feels like XyFlow".

Suggested task: `Pointer resize session parity`.

### 7. Viewport Math Is Good; `translateExtent` Is Not Enforced

Jellyflow already has renderer-neutral viewport math:

- pan and anchored zoom transforms (`crates/jellyflow-runtime/src/runtime/viewport/transform.rs:6`);
- scroll/pinch/wheel priority similar to XyFlow
  (`crates/jellyflow-runtime/src/runtime/viewport/gesture/scroll.rs:13`);
- double-click zoom animation planning
  (`crates/jellyflow-runtime/src/runtime/viewport/gesture/double_click.rs:9`);
- deterministic animation and inertia frames
  (`crates/jellyflow-runtime/src/runtime/viewport/animation.rs:66`,
  `crates/jellyflow-runtime/src/runtime/viewport/inertia.rs:32`);
- fit-view helpers with node origin and zoom clamps
  (`crates/jellyflow-runtime/src/runtime/fit_view/compute.rs:9`).

Gap: `translate_extent` appears in config
(`crates/jellyflow-runtime/src/io/config/interaction/config.rs:140`) but
`NodeGraphStore::apply_viewport_pan` and `apply_viewport_zoom` apply the raw
transform result without constraining pan
(`crates/jellyflow-runtime/src/runtime/store/view/state.rs:39`). XyFlow's d3
zoom path constrains viewport updates with `translateExtent`.

Suggested task: `Viewport translate-extent constraints`.

### 8. Auto-Pan Kernel Exists; Selection Auto-Pan Needs A Contract

Jellyflow's auto-pan is deterministic and reusable:

- `AutoPanActivation` supports node drag, connect, node focus, and `Always` for
  adapter-owned flows (`crates/jellyflow-runtime/src/runtime/auto_pan/types.rs:7`);
- `compute_auto_pan` derives per-frame screen delta from pointer proximity
  (`crates/jellyflow-runtime/src/runtime/auto_pan/planner.rs:6`);
- store application publishes normal viewport changes
  (`crates/jellyflow-runtime/src/runtime/auto_pan/store.rs:13`).

Gap: XyFlow selection auto-pan depends on a screen-space selection rectangle and
pointer/session ownership. The kernel can support it, but there is no dedicated
selection fixture or adapter-template scenario.

Suggested task: `Selection auto-pan conformance`.

### 9. Rendering Helpers Cover Nodes But Not Visible Edges

Jellyflow already covers:

- visible node IDs with viewport culling and hidden policy
  (`crates/jellyflow-runtime/src/runtime/rendering/visibility.rs:11`);
- visible node render order by composing culling and order
  (`crates/jellyflow-runtime/src/runtime/rendering/visibility.rs:82`);
- node/group/edge order with selected elevation
  (`crates/jellyflow-runtime/src/runtime/rendering/order.rs:133`,
  `crates/jellyflow-runtime/src/runtime/rendering/order.rs:182`);
- template smoke for visible node ids and visible node render order
  (`templates/headless-adapter/src/lib.rs:329`,
  `templates/headless-adapter/src/lib.rs:379`).

Gap: XyFlow has visible edge culling through `isEdgeVisible`; Jellyflow has no
equivalent `resolve_visible_edge_ids`. This belongs in `runtime::rendering`
after endpoint/path/AABB semantics are chosen.

Suggested task: `Visible edge culling contract`.

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

| Priority | Suggested Trellis Task | Owner Area | Why |
| --- | --- | --- | --- |
| P1 | Pointer resize session parity | `runtime::resize`, conformance, template | Largest system-level XyFlow semantics gap. |
| P1 | Visible edge culling contract | `runtime::rendering`, conformance | XyFlow has visible edge culling; Jellyflow only has visible nodes. |
| P1 | Viewport translate-extent constraints | `runtime::viewport`, store, conformance | Config exists but pan/zoom are not constrained like XyFlow. |
| P2 | Adapter connection target resolution fixtures | template/future adapter, conformance | Headless math exists, but rendered handle inventory and DOM-priority equivalents need contracts. |
| P2 | Async pre-delete veto and substitute delete planning | runtime/store, `runtime::xyflow` callbacks | Needed for XyFlow `onBeforeDelete` parity. |
| P2 | Selection auto-pan conformance | runtime/conformance/template | Kernel exists, but selection-specific behavior is not locked. |
| P2 | XyFlow applyChanges precision conformance | `runtime::xyflow` | Controlled integrations need exact change semantics only when adapters demand it. |
| P2 | Nested parent drag and resize semantics | runtime drag/resize/conformance | Parent-relative coordinates and child correction are likely next precision gap. |
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

The next work should be narrow parity tasks, not a rewrite: pointer resize,
visible edges, translate extents, async pre-delete, selection auto-pan, and
adapter handle-target fixtures.
