# Jellyflow Model Policy Boundary v1

Status: Closed
Last updated: 2026-05-30

## Why This Lane Exists

The public-surface refactor made the runtime package smaller, but the persisted graph model still
mixes semantic graph data, canvas layout, editor interaction policy, and XyFlow compatibility
vocabulary. Before Jellyflow becomes the stable headless substrate for Rust self-rendered adapters,
the model/policy boundary needs a clearer contract.

## Relevant Authority

- ADRs:
  - `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- Previous lane:
  - `docs/workstreams/jellyflow-runtime-public-surface-v1/CLOSEOUT_AUDIT_2026-05-30.md`
- Primary code:
  - `crates/jellyflow-core/src/core/model.rs`
  - `crates/jellyflow-core/src/ops/mod.rs`
  - `crates/jellyflow-runtime/src/io/config.rs`
  - `crates/jellyflow-runtime/src/rules/mod.rs`
  - `crates/jellyflow-runtime/src/runtime/xyflow/changes.rs`

## Problem

Jellyflow currently stores several kinds of state in the same document structs:

- semantic graph data: node kinds, ports, edges, symbols, imports, and domain payloads;
- canvas layout data: node positions/sizes, group bounds, sticky-note bounds;
- editor policy overrides: selectable, draggable, connectable, deletable, reconnectable, movement
  extent, expand-parent;
- editor visibility/presentation state: hidden, collapsed, port ordering, group/sticky colors;
- runtime-wide policy defaults in `NodeGraphInteractionState`.

This is not automatically wrong: standalone adapters need a persisted document that can round-trip
editor behavior. The risk is that callers cannot tell whether a field is semantic, layout-owned,
runtime policy, or compatibility vocabulary. As a result, rules and adapters may duplicate policy
resolution, and future schema moves become harder to justify.

## Target State

- The repository has an explicit taxonomy for semantic model, layout model, persisted editor policy,
  volatile view state, and XyFlow compatibility projection.
- Runtime exposes pure policy-resolution helpers that compute effective node/port/edge editor
  policy from graph-level overrides plus `NodeGraphInteractionState`.
- Rules/adapters call the same helpers instead of re-implementing override precedence.
- Persisted schema moves are deferred until an ADR and migration plan say which fields actually
  leave `jellyflow_core::core::Graph`.
- Existing serialization remains compatible unless a task explicitly introduces a versioned
  migration.

## In Scope

- Audit and document the model/policy taxonomy.
- Draft or update an ADR before any hard-to-reverse persisted schema movement.
- Add read-only policy-resolution APIs in `jellyflow-runtime`.
- Route connection/delete/reconnect policy checks through those APIs where behavior is already
  intended.
- Keep XyFlow compatibility changes aligned with the canonical policy vocabulary.
- Add focused tests proving override precedence and disabled-interaction behavior.

## Out Of Scope

- Extracting `jellyflow-geometry`.
- Publishing crates or changing release automation.
- Moving fields out of `Graph` without an ADR-backed migration.
- Reintroducing Fret adapter compatibility into standalone Jellyflow.
- Building UI gesture handling, DOM bindings, or renderer integrations.

## Starting Assumptions

| Assumption | Confidence | Evidence | Consequence if wrong |
| --- | --- | --- | --- |
| A read-only policy facade is the right first cleanup. | High | The model fields are already serialized and used by ops/diff/history. | If schema movement is required first, JPB-020 must promote an ADR and migration task before code edits. |
| `hidden` and `collapsed` are persisted presentation state, not volatile view state. | Medium | They affect derived geometry and graph diffs today. | If they are adapter-local, they need a migration plan instead of simple helper APIs. |
| Connection/deletion/reconnect behavior should resolve per-element overrides against global config. | High | The model comments already describe that precedence. | If adapters intentionally own those policies, runtime helpers should remain advisory only. |
| XyFlow names can remain in compatibility modules while canonical docs use Jellyflow policy terms. | High | `runtime::xyflow` is now explicit compatibility surface. | If downstream callers require old naming in core docs, migration docs are needed. |

## Architecture Direction

- Treat `jellyflow_core::core::Graph` as storage for v1, not necessarily the final semantic-only
  model.
- Add a small runtime policy module before moving storage fields. The module should be pure,
  headless, and easy for Rust adapters such as egui or fret to call.
- Prefer explicit resolved structs over scattered boolean helpers when the same policy has node,
  port, edge, and global inputs.
- Keep schema migration out of the critical path until field ownership is decided by ADR.

## Closeout Condition

This lane can close when:

- the taxonomy and ADR decision are recorded,
- effective policy resolution is implemented and tested,
- rules/adapters no longer duplicate the chosen precedence in the touched paths,
- compatibility docs/tests still pass,
- and any deferred schema migration is either split into a follow-on or marked out of scope.

Closeout result on 2026-05-30: the additive v1 boundary is implemented. Runtime policy resolution is
canonical, connect/reconnect planners use it, XyFlow compatibility docs point to it, and schema
migration/delete planner work is split into follow-ons.
