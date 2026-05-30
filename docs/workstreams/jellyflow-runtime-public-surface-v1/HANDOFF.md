# Jellyflow Runtime Public Surface v1 - Handoff

Status: Closed
Last updated: 2026-05-30

## Current State

This lane is closed for the first four fearless-refactor candidates:

1. remove runtime pass-through re-export modules,
2. isolate XyFlow compatibility,
3. split IO/config/persistence and remove Fret-era path policy,
4. deepen the store implementation while preserving the public facade.

JRP-020 is complete: runtime crate-root pass-through modules were deleted, and internal runtime
imports now reference `jellyflow_core` directly.

JRP-030 is complete: XyFlow-compatible changes, apply helpers, and callbacks now live under
`runtime::xyflow`.

JRP-040 is complete: IO/config/persistence/view-state/tuning now live in focused modules, and the
Fret-era `.fret` default path helper was removed.

JRP-050 is complete: `NodeGraphStore` remains the public facade, while dispatch, event publication,
selector/subscription handling, and view/config mutation are now private store submodules.

JRP-060 is complete: public README wording now names the explicit runtime surface, review and
verification evidence is recorded, and final gates passed.

## Final Gates

- `cargo fmt --check`: passed.
- `cargo nextest run --workspace`: passed with 115 tests.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `python3 tools/check_no_fret_dependencies.py`: passed.
- `python3 tools/check_external_consumer_smoke.py`: passed.
- Old public-path/Fret-path grep outside workstream docs: no matches.

## Decisions Since Last Update

- Opened a new durable workstream instead of using closed historical lanes.
- Kept geometry extraction and model-layer policy migration out of scope.
- Treated public API breakage as acceptable before crates.io publish, subject to focused gates.
- Removed `jellyflow-runtime::{core, interaction, ops, types}` pass-through modules.
- Moved XyFlow compatibility under `jellyflow_runtime::runtime::xyflow`.
- Split `jellyflow_runtime::io` implementation modules and removed `.fret` path policy.
- Split `NodeGraphStore` internals under private `runtime::store::{dispatch,events,subscriptions,view}` modules.

## Blockers

- None known.

## Follow-On Candidates

- Model-layer policy cleanup if semantic graph, layout, and interaction policy need sharper
  ownership boundaries.
- Geometry/spatial extraction only after at least two Rust adapters need the same pure contract.
- Downstream migration notes if consumers already depend on removed `jellyflow_runtime::{core,interaction,ops,types}` paths.
