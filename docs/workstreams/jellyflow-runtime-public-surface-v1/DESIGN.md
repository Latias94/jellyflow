# Jellyflow Runtime Public Surface v1

Status: Closed
Last updated: 2026-05-30

## Why This Lane Exists

Jellyflow has completed the first standalone extraction, but the runtime public surface still carries
extraction-era compatibility layers and XyFlow-shaped concepts in the primary path. Before the API
hardens for non-Fret consumers, the runtime should expose a smaller Rust-native headless boundary.

## Relevant Authority

- ADRs:
  - `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- Existing docs:
  - `README.md`
  - `crates/jellyflow-core/README.md`
  - `crates/jellyflow-runtime/README.md`
  - `docs/history/fret-workstreams/jellyflow-package-split-v1/`
  - `docs/history/fret-workstreams/jellyflow-standalone-readiness-v1/`
- Reference implementation:
  - `repo-ref/xyflow/packages/system/`

## Problem

The current runtime surface is correct enough to compile and smoke-test, but it is too broad for a
standalone headless Rust package:

- `jellyflow-runtime` re-exports `jellyflow-core` through shallow compatibility modules, making the
  ownership boundary ambiguous.
- Full-fidelity transaction patches and lossy XyFlow-style node/edge changes are mixed in the main
  store path.
- `io/mod.rs` mixes persistence, view state, interaction config, runtime tuning, and Fret-era path
  policy.
- `runtime/store.rs` keeps a valuable public facade but concentrates unrelated implementation
  concerns in one large file.

## Target State

- `jellyflow-core` is the only owner of core graph, interaction, ops, and type vocabulary.
- `jellyflow-runtime` exposes runtime concepts directly and imports core concepts from
  `jellyflow_core`, not through crate-local pass-through modules.
- XyFlow-compatible node/edge changes, best-effort apply helpers, and ReactFlow-style callback
  aliases live behind an explicit compatibility module.
- IO/config/persistence modules are split by responsibility, and Fret-specific default paths are
  removed from Jellyflow.
- `NodeGraphStore` remains the public entry point while implementation details move into private
  store submodules.
- Focused tests and external consumer smoke gates prove the headless contract still works.

## In Scope

- Public-surface cleanup in `crates/jellyflow-runtime`.
- Internal import cleanup after removing runtime pass-through modules.
- Compatibility-module extraction for XyFlow-style changes, apply helpers, and callback aliases.
- IO module split and removal/replacement of the `.fret` editor-state default path helper.
- Store implementation split that preserves the public `NodeGraphStore` facade.
- README/example updates required by the new module paths.

## Out Of Scope

- Moving graph model fields between semantic graph, layout, and interaction policy.
- Creating `jellyflow-geometry`.
- Porting DOM/d3-dependent XyFlow drag, panzoom, handle, minimap, or resizer modules.
- Publishing crates or changing release automation.
- Reintroducing Fret adapter compatibility paths in this standalone repository.

## Starting Assumptions

| Assumption | Confidence | Evidence | Consequence if wrong |
| --- | --- | --- | --- |
| Public API breakage is acceptable before crates.io publish. | High | README says publishing is blocked until metadata, CI, package lists, and dry-runs are verified. | If compatibility is required, removals need deprecation aliases instead of direct deletion. |
| `jellyflow_runtime::{core, interaction, ops, types}` are extraction leftovers, not intended long-term API. | Medium | They are direct `pub use jellyflow_core::*` wrappers in `crates/jellyflow-runtime/src/lib.rs`. | If downstream code relies on them, a migration note or prelude may be needed. |
| XyFlow-style node/edge changes are adapter compatibility, not the canonical Rust write path. | High | `NodeGraphPatch` preserves all graph resources; `NodeGraphChanges` is documented as lossy. | If controlled-mode consumers require node/edge changes by default, the store facade may keep convenience projection methods. |
| `.fret` path policy belongs outside standalone Jellyflow. | High | ADR 0001 says Fret conversions and integration belong in the Fret adapter. | If standalone Jellyflow intentionally owns a default project layout, it needs a Jellyflow-branded path and ADR. |
| Store implementation can be split without changing public methods. | High | `NodeGraphStore` already keeps most fields private. | If borrow/lifetime constraints make a split noisy, the task can stop at private helper modules with no API churn. |

## Architecture Direction

The lane deepens modules by shrinking interfaces before adding new capability:

- Delete pass-through modules when deletion reduces total caller knowledge.
- Make the compatibility seam explicit only where there is a real adapter vocabulary: XyFlow-style
  change arrays and callback aliases.
- Keep `NodeGraphStore` as a deep public module but move implementation concerns behind private
  store modules.
- Keep geometry and interaction-kernel extraction deferred until at least two Rust adapters need the
  same pure contract.

## Closeout Condition

This lane can close when:

- JRP-020 through JRP-050 are implemented or explicitly split into follow-ons,
- targeted and package gates pass with fresh evidence,
- docs and examples use the new public paths,
- no Fret path policy remains in Jellyflow runtime,
- and remaining API/model risks are recorded for follow-up.

Closeout result on 2026-05-30: all lane tasks are complete, final gates passed, and remaining
model/geometry work is deferred to separate follow-on lanes.
