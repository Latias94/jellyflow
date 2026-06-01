# Jellyflow Node Drag Module Split v1 - Design

Status: Closed
Date: 2026-06-01

## Problem

`runtime::drag` owns the public node-drag request/plan API, deterministic multi-node planning,
selection candidate filtering, snap-grid behavior, extent clamping, group bounds, and
`NodeGraphStore` extension methods in one file. The behavior is tested and useful, but the module
boundary is now too broad before parent expansion, resize, or richer drag fixtures are added.

## Target State

- `runtime::drag::mod` becomes a small facade that preserves the existing public API.
- Public request/plan/item types are separate from planner execution.
- Candidate selection and policy filtering live separately from constraint math.
- Store extension methods live separately from pure planning helpers.
- Existing drag tests, conformance fixtures, and public-surface smoke tests keep passing.

## Scope

- Split `crates/jellyflow-runtime/src/runtime/drag.rs` into `runtime/drag/` submodules.
- Preserve `jellyflow_runtime::runtime::drag::*` public paths and crate-root behavior.
- Keep node drag transaction semantics, ordering, snap, extent, and store dispatch behavior
  unchanged.
- Update workstream evidence and closeout docs.

## Non-Goals

- No parent expansion behavior changes.
- No resize, reconnect, pan/zoom, pointer capture, DOM, renderer, or adapter code.
- No fixture schema changes.
- No new dependencies.

## Architecture Direction

Use a facade plus owned submodules:

1. `mod.rs`: documentation, module declarations, and public re-exports.
2. `types.rs`: `NODE_DRAG_TRANSACTION_LABEL`, `NodeDragRequest`, `NodeDragItem`,
   `NodeDragPlan`.
3. `planner.rs`: public `plan_node_drag` and transaction construction.
4. `candidates.rs`: selected-node candidate discovery and drag eligibility.
5. `constraints.rs`: snap-grid, bounds, extent, clamp, size normalization, and resolved extents.
6. `store.rs`: `NodeGraphStore` node-drag extension methods.

The split is behavior-preserving. If public-surface tests fail, prefer explicit re-exports over
changing call sites.

## Outcome

Closed on 2026-06-01. `runtime::drag` is now a facade over focused `types`, `planner`,
`candidates`, `constraints`, and `store` submodules. Public `runtime::drag::*` paths, store
extension methods, node drag transaction behavior, snap-grid behavior, extent clamping, and
conformance traces remain unchanged.
