# Jellyflow Visible Elements Contract v1

Status: Closed
Last updated: 2026-06-02

## Why This Lane Exists

Jellyflow already has the pure geometry primitive needed for XyFlow-style visible node filtering:
`runtime::utils::get_nodes_inside`. It also persists `only_render_visible_elements` and
`NodeGraphSpatialIndexTuning`, but adapters still have to assemble viewport size, view-state
transform, node-origin policy, hidden-node policy, fallback size policy, and sorted IDs manually.

XyFlow exposes this behavior as adapter-visible rendering policy:

- `onlyRenderVisibleElements` switches between all nodes and viewport-filtered nodes;
- `useVisibleNodeIds` calls `getNodesInside(nodeLookup, viewportRect, transform, true)`;
- `getNodesInside` converts screen viewport bounds into renderer/canvas coordinates, skips hidden
  nodes, and includes partially visible nodes.

Jellyflow should provide the same headless contract without adding a renderer or a real spatial
index yet. The stable seam should be usable by egui, wgpu, Fret, and server-side tests before
future work swaps the linear scan for an indexed backend.

## Target State

- A renderer-neutral runtime module exposes visible node id planning from a viewport transform and
  logical viewport size.
- `NodeGraphStore` exposes a helper that uses current view state and resolved interaction/runtime
  tuning, including `only_render_visible_elements`.
- The first implementation remains deterministic linear scan with sorted output.
- Conformance/template coverage proves the adapter-facing trace can assert visible node ids before
  renderer smoke tests.
- Real spatial indexing remains a future backend behind the same contract, only after workload
  evidence justifies it.

## Scope

In scope:

- visible node ids, not visible edge ids;
- current viewport pan/zoom and logical viewport size;
- `only_render_visible_elements` false/true behavior;
- hidden nodes, missing sizes, fallback size, node origin, and deterministic sorted output;
- conformance action and template smoke coverage for visible node ids.

Out of scope:

- visible edge ids and edge AABB/path culling;
- a real spatial index, grid, R-tree, or quadtree backend;
- renderer frame scheduling, viewport resize observers, screenshots, and pixels;
- schema migration for runtime tuning fields.

## Source Coverage

| Source | State | Evidence | Impact |
| --- | --- | --- | --- |
| User goal | COVERED | ongoing `$improve-codebase-architecture` + `$fearless-refactor` lane | Continue headless XyFlow-feel contracts. |
| Repo context | COVERED | `CONTEXT.md` | Real spatial indexing is a follow-on after workload evidence; this lane creates the stable contract first. |
| ADRs | COVERED | ADR 0001, ADR 0003 | Runtime owns headless behavior; renderer smoke stays outside runtime. |
| Prior workstream | COVERED | `jellyflow-geometry-spatial-v1` | `get_nodes_inside` exists and real indexing was intentionally deferred. |
| XyFlow source | COVERED | `repo-ref/xyflow/packages/react/src/hooks/useVisibleNodeIds.ts`, `repo-ref/xyflow/packages/system/src/utils/graph.ts`, `repo-ref/xyflow/packages/react/src/types/component-props.ts` | Defines `onlyRenderVisibleElements` and visible node filtering semantics. |
| Jellyflow code | COVERED | `runtime::utils::get_nodes_inside`, `runtime::viewport::ViewportTransform`, `NodeGraphInteractionState::rendering_interaction` | Existing pieces can be deepened into one adapter-facing seam. |

## Architecture Direction

The deepened module should hide composition complexity behind a small interface. Adapters should not
need to know how `NodeGraphViewState`, `ViewportTransform`, `GetNodesInsideOptions`, and rendering
tuning combine.

The v1 seam should:

- accept a logical viewport size from the adapter;
- derive the current viewport transform from view state;
- return all visible candidate node ids when culling is disabled;
- return partially visible node ids when culling is enabled;
- keep output deterministic and sorted;
- expose enough request/options structure that a future spatial index can replace the linear scan
  without changing adapter calls.

## Task Plan

- JVE-010 opens the workstream and freezes source coverage.
- JVE-020 adds the runtime/store visible node id contract and focused tests.
- JVE-030 adds conformance/template coverage for adapter-facing visible node assertions.
- JVE-040 updates docs, records closeout evidence, and splits visible edge ids/spatial index work.

## Closeout Condition

This lane can close when:

- adapters have a public headless helper for visible node ids;
- store helper behavior respects `only_render_visible_elements`;
- conformance/template smoke can assert visible node ids;
- docs explain why real spatial indexing and visible edge ids remain follow-ons;
- focused runtime, template, package, JSON, and diff gates pass.
