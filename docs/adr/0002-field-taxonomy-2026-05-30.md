# ADR 0002 Field Taxonomy

Date: 2026-05-30

## Decision

ADR 0002 accepts an additive v1 boundary:

- keep existing persisted fields in `jellyflow_core::core::Graph`,
- document their ownership/lifecycle explicitly,
- add runtime policy-resolution helpers before any schema movement,
- defer field migration to a separate ADR-backed follow-on.

## Categories

| Category | Meaning | Owner |
| --- | --- | --- |
| Semantic model | Domain graph identity, typing, connectivity, and payloads. | `jellyflow-core` |
| Layout model | Canvas-space placement and persisted structural layout. | `jellyflow-core` storage, runtime/adapters consume |
| Persisted editor policy | Per-element overrides for editor interaction behavior. | `jellyflow-core` storage, `jellyflow-runtime::policy` resolves |
| Persisted presentation | Document-visible presentation flags or styling. | `jellyflow-core` storage, adapters/renderers consume |
| Volatile/per-user view state | Pan, zoom, selection, node/edge/group draw order, and runtime/editor defaults stored outside the graph. | `jellyflow-runtime::io` |
| Compatibility vocabulary | XyFlow/ReactFlow-shaped change and callback naming. | `jellyflow-runtime::runtime::xyflow` |

## Graph-Level Fields

| Field | Category | Notes |
| --- | --- | --- |
| `graph_id` | Semantic model | Stable document identity and cross-graph reference anchor. |
| `graph_version` | Semantic model | Version for migrations. |
| `imports` | Semantic model | Graph dependency/import contract. |
| `symbols` | Semantic model | Blackboard/variable model. |
| `nodes` | Semantic model container | Contains mixed node fields listed below. |
| `ports` | Semantic model container | Contains mixed port fields listed below. |
| `edges` | Semantic model container | Contains mixed edge fields listed below. |
| `groups` | Layout model | Editor grouping/frame structure. Not semantic subgraphs. |
| `sticky_notes` | Persisted presentation | Editor annotation surface. |

## Node Fields

| Field | Category | Notes |
| --- | --- | --- |
| `kind` | Semantic model | Node kind identifier. |
| `kind_version` | Semantic model | Per-kind migration/version anchor. |
| `data` | Semantic model | Domain-owned opaque payload. |
| `pos` | Layout model | Canvas placement. |
| `origin` | Layout model | Per-node override resolved against global `node_origin` for bounds and drag math. |
| `size` | Layout model | Explicit measured/logical size. |
| `parent` | Layout model | Group/container relationship, not semantic subgraph. |
| `ports` | Persisted presentation/layout | Stable UI ordering for semantic ports. |
| `selectable` | Persisted editor policy | Override resolved against global `elements_selectable`. |
| `draggable` | Persisted editor policy | Override resolved against global `nodes_draggable`. |
| `connectable` | Persisted editor policy | Override resolved with port policy and global `nodes_connectable`. |
| `deletable` | Persisted editor policy | Override resolved against global `nodes_deletable`. |
| `extent` | Persisted editor policy | Movement/resize constraint; stored in graph but resolved by runtime. |
| `expand_parent` | Persisted editor policy | Container expansion permission. |
| `hidden` | Persisted presentation | Affects derived geometry/rendering and should remain document-visible for v1. |
| `collapsed` | Persisted presentation | Document-visible presentation state. |

## Port Fields

| Field | Category | Notes |
| --- | --- | --- |
| `node` | Semantic model | Owning node id. |
| `key` | Semantic model | Stable schema key. |
| `dir` | Semantic model | Input/output direction. |
| `kind` | Semantic model | Data/exec port kind. |
| `capacity` | Semantic model | Single/multi connection capacity. |
| `ty` | Semantic model | Optional type descriptor. |
| `data` | Semantic model | Domain-owned opaque payload. |
| `connectable` | Persisted editor policy | General port connectability override. |
| `connectable_start` | Persisted editor policy | Start-side connection override. |
| `connectable_end` | Persisted editor policy | End-side connection override. |

## Edge Fields

| Field | Category | Notes |
| --- | --- | --- |
| `kind` | Semantic model | Data/exec edge kind. |
| `from` | Semantic model | Source port. |
| `to` | Semantic model | Target port. |
| `hidden` | Persisted presentation | Excluded from derived selection/rendering surfaces. |
| `selectable` | Persisted editor policy | Override resolved against global `edges_selectable`. |
| `focusable` | Persisted editor policy | Override resolved against global `edges_focusable`. |
| `interaction_width` | Persisted editor policy | Override resolved against global `edge_interaction_width` for hit testing. |
| `deletable` | Persisted editor policy | Override resolved against global `edges_deletable`. |
| `reconnectable` | Persisted editor policy | Override resolved against global `edges_reconnectable`. |

## Runtime Config And View State

| Field group | Category | Notes |
| --- | --- | --- |
| `NodeGraphPureViewState::{pan, zoom, selected_*, draw_order, edge_draw_order, group_draw_order}` | Volatile/per-user view state | Stored outside `Graph`; may be per project or per user. |
| `NodeGraphInteractionConfig` | Volatile/per-user or editor profile policy defaults | Global defaults for effective editor behavior. |
| `NodeGraphRuntimeTuning` | Runtime tuning | Performance/cache/spatial index settings, including reserved backend payloads; not graph semantics or proof that a backend is active. |
| `NodeGraphInteractionState` | Resolved global policy state | Input to runtime policy facade. |

## Policy Resolution Rules For Follow-On Work

The next policy task should implement these as pure runtime helpers:

- Node selection: `node.selectable.unwrap_or(state.elements_selectable)`.
- Node drag: `node.draggable.unwrap_or(state.nodes_draggable)`.
- Node deletion: `node.deletable.unwrap_or(state.nodes_deletable)`.
- Node connection participation: `node.connectable.unwrap_or(state.nodes_connectable)`.
- Port connection participation: combine node connection participation with
  `port.connectable.unwrap_or(true)`.
- Port start/end participation: combine port participation with
  `port.connectable_start.unwrap_or(true)` or `port.connectable_end.unwrap_or(true)`.
- Edge selection: `edge.selectable.unwrap_or(state.edges_selectable)`.
- Edge hit testing: `edge.interaction_width.unwrap_or(state.edge_interaction_width)`.
- Edge deletion: `edge.deletable.unwrap_or(state.edges_deletable)`.
- Edge reconnection: resolve `edge.reconnectable` against `state.edges_reconnectable`, preserving
  endpoint-specific variants.

## Non-Decisions

- No field is moved out of `Graph` in the ADR 0002 taxonomy.
- No graph file version is changed in the ADR 0002 taxonomy.
- No behavior enforcement is changed by this taxonomy.
