# Jellyflow Runtime Public Surface v1 - TODO

Status: Closed
Last updated: 2026-05-30

## M0 - Scope And Evidence Freeze

- [x] JRP-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-runtime-public-surface-v1]
  Goal: Freeze problem, target state, non-goals, and evidence anchors for the public-surface refactor.
  Validation: DESIGN.md, MILESTONES.md, EVIDENCE_AND_GATES.md, WORKSTREAM.json, and HANDOFF.md exist and agree.
  Evidence: `docs/workstreams/jellyflow-runtime-public-surface-v1/DESIGN.md`
  Handoff: Planner opened the lane before code edits.

## M1 - Public Surface Shrink

- [x] JRP-020 [owner=codex] [deps=JRP-010] [scope=crates/jellyflow-runtime/src/lib.rs,crates/jellyflow-runtime/src/** imports,README/examples]
  Goal: Remove runtime `core`, `interaction`, `ops`, and `types` compatibility re-export modules and update internal/example imports to use `jellyflow_core` directly.
  Validation: `cargo check -p jellyflow-runtime`; `cargo nextest run -p jellyflow-runtime`; `python3 tools/check_external_consumer_smoke.py`
  Review: review-workstream before accepting completion.
  Evidence: `crates/jellyflow-runtime/src/lib.rs`, README/example imports, external smoke output.
  Handoff: DONE 2026-05-30. Deleted crate-root pass-through modules and updated runtime imports to `jellyflow_core`.

- [x] JRP-030 [owner=codex] [deps=JRP-020] [scope=crates/jellyflow-runtime/src/runtime/{changes,apply,callbacks,store,events}.rs,tests]
  Goal: Move XyFlow-compatible node/edge changes, best-effort apply helpers, and ReactFlow-style callback aliases behind an explicit compatibility module while keeping full graph patches primary.
  Validation: `cargo nextest run -p jellyflow-runtime runtime`; `cargo check -p jellyflow-runtime`
  Review: review-workstream before accepting completion.
  Evidence: compatibility module tests and store dispatch tests.
  Handoff: DONE 2026-05-30. Moved compatibility files under `runtime::xyflow` and documented the new module home.

## M2 - IO And Store Deepening

- [x] JRP-040 [owner=codex] [deps=JRP-020] [scope=crates/jellyflow-runtime/src/io/**,README/examples]
  Goal: Split IO/config/persistence/view-state/tuning responsibilities and remove Fret-era `.fret` default path policy from Jellyflow runtime.
  Validation: `cargo nextest run -p jellyflow-runtime io`; `cargo check -p jellyflow-runtime`; `python3 tools/check_external_consumer_smoke.py`
  Review: review-workstream before accepting completion.
  Evidence: IO module tests and updated docs/examples.
  Handoff: DONE 2026-05-30. Split IO modules and removed Fret-era default path helper; no Jellyflow-branded default path was introduced.

- [x] JRP-050 [owner=codex] [deps=JRP-030] [scope=crates/jellyflow-runtime/src/runtime/store/**,crates/jellyflow-runtime/src/runtime/store.rs,tests]
  Goal: Preserve `NodeGraphStore` as the public facade while moving dispatch, view/config mutation, subscription, and event internals into private store submodules.
  Validation: `cargo nextest run -p jellyflow-runtime runtime`; `cargo check -p jellyflow-runtime`
  Review: review-workstream before accepting completion.
  Evidence: store dispatch/subscription/view tests.
  Handoff: DONE 2026-05-30. Preserved the `NodeGraphStore` facade while moving dispatch, event publication, selector/subscription handling, and view/config mutation into private store submodules.

## M3 - Integration And Closeout

- [x] JRP-060 [owner=codex] [deps=JRP-040,JRP-050] [scope=README.md,crates/jellyflow-runtime/README.md,docs/workstreams/jellyflow-runtime-public-surface-v1]
  Goal: Update docs and record fresh final gate evidence for the completed refactor lane.
  Validation: `cargo fmt --check`; `cargo nextest run --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `python3 tools/check_no_fret_dependencies.py`; `python3 tools/check_external_consumer_smoke.py`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: `EVIDENCE_AND_GATES.md`, `HANDOFF.md`, final command output.
  Handoff: DONE 2026-05-30. Updated runtime docs, ran review/verification gates, recorded closeout evidence, and closed the lane. Split model-layer policy cleanup or geometry extraction into follow-ons if still relevant.
