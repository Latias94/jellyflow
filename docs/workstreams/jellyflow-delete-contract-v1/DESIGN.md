# Jellyflow Delete Contract v1

Status: Closed
Last updated: 2026-06-02

## Why This Lane Exists

Jellyflow already has renderer-neutral delete selection helpers under `runtime::delete` and a
keyboard intent router under `runtime::keyboard`, but the architecture docs still carry older
follow-on language that says delete planner ownership is missing. That drift weakens the headless
contract: adapters can see delete conformance actions, but the template suite does not yet prove
delete selection as a first-class smoke scenario.

XyFlow delete behavior is adapter-visible rather than purely model-level:

- key handlers route selected nodes and edges into `deleteElements`;
- `deleteElements` resolves deletable nodes, cascaded connected edges, and delete callbacks;
- `onNodesDelete`, `onEdgesDelete`, and combined `onDelete` callbacks are part of the feel.

Jellyflow should keep the DOM key handler and renderer ownership outside the headless crates, while
making the stable selection-delete planner, key-bound gate, graph commit, view-state cleanup, and
callback trace an explicit adapter contract.

## Target State

- `runtime::delete` and `runtime::keyboard` are documented as the canonical headless delete seam.
- Template adapter smoke includes a delete selection scenario with XyFlow-style callback trace.
- Conformance fixtures continue to prefer `apply_delete_selection` or
  `apply_delete_selection_for_key` over raw transactions for adapter feel.
- Legacy follow-on language is updated so future agents do not re-open "no delete planner exists"
  as a stale problem.
- No DOM, renderer, `wgpu`, egui, Fret, screenshot, or pixel dependency enters
  `jellyflow-core` or `jellyflow-runtime`.

## Scope

In scope:

- evidence audit for current delete planner and keyboard intent behavior;
- adapter template delete smoke scenario and exported smoke helper;
- README/runtime README/CONTEXT wording for delete runtime and adapter responsibilities;
- workstream evidence, closeout, and follow-on split.

Out of scope:

- browser key handling, focus traps, and raw DOM events;
- async `onBeforeDelete` parity;
- renderer confirmation dialogs;
- schema migration for persisted `deletable` policy fields;
- group/sticky-note deletion expansion beyond the current node/edge selection contract.

## Source Coverage

| Source | State | Evidence | Impact |
| --- | --- | --- | --- |
| User goal | COVERED | ongoing `$improve-codebase-architecture` + `$fearless-refactor` lane | Continue headless XyFlow-feel refactors autonomously. |
| Repo context | COVERED | `CONTEXT.md` | Delete planner ownership was a named follow-on and should be resolved. |
| ADRs | COVERED | ADR 0001, 0002, 0003 | Runtime owns headless behavior; renderer and platform stay outside. |
| Prior workstream | COVERED | `jellyflow-model-policy-boundary-v1` | Old closeout named delete planner ownership as a follow-on. |
| XyFlow source | COVERED | `repo-ref/xyflow/packages/system/src/utils/graph.ts`, `repo-ref/xyflow/packages/react/src/hooks/useReactFlow.ts`, `repo-ref/xyflow/packages/react/src/hooks/useGlobalKeyHandler.ts` | Delete selection, cascaded edges, and callback ordering are adapter-visible feel. |
| Jellyflow code | COVERED | `runtime::delete`, `runtime::keyboard`, `runtime::conformance` | The core seam exists; template/docs need promotion. |

## Architecture Direction

The deepened module is not a new trait or adapter interface. The correct seam already exists:
`NodeGraphStore::{plan_delete_selection, apply_delete_selection, apply_delete_selection_for_key}`
plus `KeyboardIntent`. This lane should increase leverage by making that small interface cover the
visible delete behavior adapters need:

- selection lookup comes from `NodeGraphViewState`;
- effective `deletable` policy is resolved through `NodeGraphInteractionState`;
- accepted plans become normal `GraphTransaction` commits;
- store dispatch sanitizes stale selection and draw order;
- conformance traces expose the graph and callback ordering.

Adapters still translate platform keys into `KeyboardIntent` and may own confirmation dialogs or
async pre-delete hooks before calling Jellyflow.

## Task Plan

- JDC-010 opens the workstream and freezes source coverage.
- JDC-020 promotes delete selection into the template adapter smoke suite.
- JDC-030 updates public docs, records final evidence, and closes or splits follow-ons.

## Closeout Condition

This lane can close when:

- template adapter `check` includes delete selection and matches expected traces;
- runtime/conformance delete tests still pass;
- docs teach the delete runtime/adapter split;
- stale "no delete planner" follow-on wording is no longer the current navigation state;
- package, JSON, and diff gates pass.

Closed on 2026-06-02. Delete selection is documented as a runtime-owned headless contract, the
template suite includes key-bound delete selection coverage, and async pre-delete/confirmation UI
parity remains split to adapters or future evidence-backed lanes.
