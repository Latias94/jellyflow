# ADR 0002: Jellyflow Model Policy Boundary

Status: Accepted
Date: 2026-05-30

## Context

Jellyflow is becoming a reusable headless node/flow graph substrate for Rust adapters. The first
public-surface cleanup removed extraction-era runtime re-exports and isolated XyFlow compatibility,
but the persisted graph document still carries several lifecycles in one model:

- semantic graph data, such as node kinds, ports, edges, symbols, imports, and domain payloads,
- canvas layout data, such as node positions, node sizes, group bounds, and sticky-note bounds,
- persisted editor policy overrides, such as selectable, focusable, draggable, connectable,
  deletable, reconnectable, extent, and expand-parent,
- presentation state, such as hidden, collapsed, color, and port ordering,
- volatile editor view state and runtime-wide defaults stored outside the graph in
  `jellyflow-runtime`.

Moving all non-semantic fields out of `jellyflow_core::core::Graph` immediately would be a hard
schema break and would risk losing useful standalone editor behavior. At the same time, leaving the
field ownership implicit makes it hard for future Rust adapters to know which layer decides
interaction behavior.

## Decision

Keep `jellyflow_core::core::Graph` as the v1 persisted document shape. Do not move persisted policy,
layout, or presentation fields out of `Graph` in the model-policy boundary lane.

Instead, make ownership explicit and add a canonical policy-resolution layer in
`jellyflow-runtime`:

- `jellyflow-core` owns storage and undoable graph edits.
- `jellyflow-runtime::io::NodeGraphInteractionConfig` and `NodeGraphInteractionState` own global
  editor policy defaults.
- a new runtime policy facade will resolve effective node, port, and edge interaction policy from
  graph-local overrides plus global runtime state.
- runtime rules and adapters should use that facade instead of duplicating override precedence.
- `runtime::xyflow` remains the compatibility home for XyFlow-shaped change names and callbacks.

Schema migration remains a separate follow-on. A future migration may split persisted editor policy
or layout into separate files, but only after the runtime facade proves the intended behavior and a
versioned migration plan exists.

## Field Taxonomy

The detailed field inventory lives in
`docs/adr/0002-field-taxonomy-2026-05-30.md`.

The summary taxonomy is:

- semantic model: graph identity/version, imports, symbols, node kind/version/data, port key,
  direction, kind, capacity, type/data, edge kind/endpoints;
- layout model: node position/origin/size, group bounds, sticky-note bounds, parent group relationship;
- persisted editor policy: node selectable/focusable/draggable/connectable/deletable/extent/expand-parent,
  port connectable/start/end, edge selectable/focusable/interaction-width/deletable/reconnectable;
- persisted presentation: node/edge hidden, collapsed, port order, group/sticky color;
- volatile or per-user view state: pan, zoom, selection, node/edge/group draw order, and
  editor/runtime config files.

## Consequences

- The next code slice can be additive and non-breaking.
- `Graph` remains broader than a pure semantic model, but its broader role is explicit: v1 storage
  for semantic, layout, policy override, and presentation data.
- Adapters get one policy-resolution contract to share.
- Future schema migration has a clearer target and can be split from behavior cleanup.
- XyFlow naming stays contained in compatibility modules and should not drive canonical Jellyflow
  docs.

## Follow-Up

- Add `jellyflow-runtime::runtime::policy` with pure policy-resolution APIs.
- Route intended connect/delete/reconnect checks through the facade.
- Add tests for global default, per-element override, and disabled override precedence.
- Revisit schema movement only after the facade and behavior tests are stable.
