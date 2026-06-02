# Jellyflow Model Policy Boundary v1 - Handoff

Status: Closed
Last updated: 2026-05-30

## Current State

This lane is closed. It was opened as a follow-on to `jellyflow-runtime-public-surface-v1`.

The public-surface lane closed with an explicit follow-on candidate for model-layer policy cleanup.
Initial inspection shows `jellyflow_core::core::{Node, Port, Edge}` stores a mix of semantic graph
data, canvas layout, persisted editor policy overrides, and presentation flags. Runtime config owns
global interaction defaults, but there is no single public policy-resolution facade yet.

JPB-020 is complete: ADR 0002 accepts an additive v1 boundary. Existing persisted fields stay in
`Graph`; the next code slice adds runtime policy-resolution helpers before any schema migration.

JPB-030 is complete: `runtime::policy` exposes pure node, port, and edge interaction policy
resolution from per-element overrides plus `NodeGraphInteractionState`.

JPB-040 is complete with a named concern: connect/reconnect planners now route through
`runtime::policy`, but delete enforcement was not changed because runtime has no delete planner path
yet. `runtime::policy` exposes node/edge `deletable` state for adapters.

JPB-050 is complete: public README wording and XyFlow module docs point effective interaction policy
resolution to `runtime::policy` and keep XyFlow naming in the compatibility module.

JPB-060 is complete: final gates passed and the lane is closed.

## Final Gates

- `cargo fmt --check`: passed.
- `cargo nextest run --workspace`: passed with 120 tests.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `python3 tools/check_no_fret_dependencies.py`: passed.
- `python3 tools/check_external_consumer_smoke.py`: passed.
- `jq empty docs/workstreams/jellyflow-model-policy-boundary-v1/WORKSTREAM.json`: passed.
- `git diff --check`: passed.

## Decisions Since Last Update

- Chose model/policy cleanup as the next lane instead of geometry extraction because geometry still
  needs a second adapter-driven contract.
- Kept persisted schema migration out of the first executable task.
- Chose an additive policy-resolution facade as the likely first code slice after the taxonomy
  decision.
- Added ADR 0002 and a field taxonomy note.
- Decided v1 keeps existing persisted fields in `Graph`; schema migration is a follow-on after the
  runtime facade proves behavior.
- Verified JPB-020 with `cargo fmt --check`, `git diff --check`, and `jq empty` for WORKSTREAM.json.
- Self-reviewed JPB-020 with no blocking findings.
- Added `runtime::policy` read-only helpers and tests for override precedence.
- Verified JPB-030 with `cargo check -p jellyflow-runtime`, `cargo nextest run -p jellyflow-runtime policy`, and package clippy.
- Added policy-aware connect/reconnect planning variants and made default planners respect persisted
  connectable/reconnectable overrides.
- Verified JPB-040 with runtime/rules nextest filters, package check, and package clippy.
- Updated README and `runtime::xyflow` docs to make `runtime::policy` the canonical policy
  resolution entry point.
- Closed the lane after final gates.

## Blockers

- None known.

## Follow-On Candidates

- Open a delete-planner lane if runtime should own deletion eligibility and delete transaction
  planning instead of leaving it to adapters.
- Open a schema-migration lane only after policy facade usage proves which persisted fields should
  leave `jellyflow_core::core::Graph`.
- Keep geometry/spatial extraction separate until at least two Rust adapters need the same pure
  contract.
