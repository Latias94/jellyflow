# Jellyflow Runtime Public Surface v1 - Closeout Audit

Date: 2026-05-30

## Final Status

Closed. JRP-020 through JRP-060 are complete.

## Completed Outcomes

- Removed `jellyflow-runtime::{core, interaction, ops, types}` pass-through modules.
- Moved XyFlow-compatible changes, apply helpers, and callbacks under `runtime::xyflow`.
- Split IO/config/persistence/view-state/tuning into focused modules and removed the Fret-era
  `.fret` default path helper.
- Preserved `NodeGraphStore` as the public facade while moving dispatch, events, subscriptions, and
  view/config mutation into private store submodules.
- Updated README wording to describe the shipped runtime surface.

## Review

`review-workstream` self-review found no blocking findings.

- Workstream compliance: target state, task dependencies, docs, and ADR 0331 boundary are satisfied.
- Code quality: new modules are private where intended, public compatibility vocabulary is explicit,
  and no public store trait proliferation was introduced.
- Missing gates: none after closeout verification.
- Residual risk: API breakage is intentional before crates.io publish; downstream callers using old
  pass-through paths need migration to `jellyflow_core` and `runtime::xyflow`.

## Verification

`verify-rust-workstream` closeout claim: the runtime public surface refactor is implemented and the
workspace remains buildable, tested, lint-clean, and free of Fret runtime dependencies.

- `rg -n "runtime::(changes|apply|callbacks)|jellyflow_runtime::(core|interaction|ops|types)|default_project_editor_state_path|\\.fret" --glob '!docs/workstreams/**' --glob '!repo-ref/**'`: no matches.
- `cargo fmt --check`: passed.
- `cargo nextest run --workspace`: passed with 115 tests.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `python3 tools/check_no_fret_dependencies.py`: passed.
- `python3 tools/check_external_consumer_smoke.py`: passed.
- `jq empty docs/workstreams/jellyflow-runtime-public-surface-v1/WORKSTREAM.json`: passed after
  closeout edits.
- `git diff --check`: passed after closeout edits.

## Follow-Ons

- Model-layer policy cleanup if semantic graph, layout, and interaction policy need sharper
  ownership boundaries.
- Geometry/spatial extraction only after at least two Rust adapters need the same pure contract.
- Downstream migration notes if users already depend on removed pre-publish runtime pass-through
  paths.
