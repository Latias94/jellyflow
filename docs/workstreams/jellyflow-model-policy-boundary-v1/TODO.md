# Jellyflow Model Policy Boundary v1 - TODO

Status: Closed
Last updated: 2026-05-30

## M0 - Scope And Evidence Freeze

- [x] JPB-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-model-policy-boundary-v1]
  Goal: Open the follow-on lane, freeze the problem statement, and identify the first executable slice.
  Validation: DESIGN.md, MILESTONES.md, EVIDENCE_AND_GATES.md, WORKSTREAM.json, and HANDOFF.md exist and agree.
  Evidence: `docs/workstreams/jellyflow-model-policy-boundary-v1/DESIGN.md`
  Handoff: DONE 2026-05-30. Opened from the public-surface closeout follow-on list.

## M1 - Taxonomy And Decision Record

- [x] JPB-020 [owner=codex] [deps=JPB-010] [scope=docs/adr,docs/workstreams/jellyflow-model-policy-boundary-v1,crates/jellyflow-core/src/core/model.rs,crates/jellyflow-runtime/src/io/config.rs]
  Goal: Classify existing graph/config fields as semantic model, layout model, persisted editor policy, volatile view state, or XyFlow compatibility vocabulary; record whether v1 keeps fields in place or prepares a migration.
  Validation: taxonomy note or ADR exists; `cargo fmt --check`; `git diff --check`.
  Review: review-workstream before accepting completion.
  Evidence: ADR/taxonomy document and field inventory.
  Handoff: DONE 2026-05-30. ADR 0332 accepts an additive v1 boundary: keep persisted fields in `Graph`, add runtime policy-resolution helpers next, and defer schema migration to a later ADR-backed follow-on.

## M2 - Policy Resolution Facade

- [x] JPB-030 [owner=codex] [deps=JPB-020] [scope=crates/jellyflow-runtime/src/runtime/policy.rs,crates/jellyflow-runtime/src/runtime/mod.rs,tests]
  Goal: Add pure runtime policy-resolution APIs for effective node, port, and edge interaction policy using graph overrides plus `NodeGraphInteractionState`.
  Validation: `cargo check -p jellyflow-runtime`; `cargo nextest run -p jellyflow-runtime policy`.
  Review: review-workstream before accepting completion.
  Evidence: policy resolution tests for global default, per-element override, and disabled override precedence.
  Handoff: DONE 2026-05-30. Added read-only `runtime::policy` helpers for node, port, and edge interaction policy; tests cover global defaults, per-element overrides, disabled precedence, and endpoint reconnectability.

- [x] JPB-040 [owner=codex] [deps=JPB-030] [scope=crates/jellyflow-runtime/src/rules/**,crates/jellyflow-runtime/src/runtime/**,tests]
  Goal: Route intended connect/delete/reconnect policy checks through the new facade and add tests for disabled interactions.
  Validation: `cargo nextest run -p jellyflow-runtime rules`; `cargo nextest run -p jellyflow-runtime runtime`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: rules/runtime tests showing the same override precedence is used by behavior and snapshots.
  Handoff: DONE_WITH_CONCERNS 2026-05-30. Connected connect/reconnect planners to `runtime::policy` and added disabled-interaction tests. Delete enforcement was not changed because runtime currently has no delete planner path; `runtime::policy` exposes deletable state for adapters, and a delete planner should be split if needed.

## M3 - Compatibility And Closeout

- [x] JPB-050 [owner=codex] [deps=JPB-030] [scope=crates/jellyflow-runtime/src/runtime/xyflow/**,README.md,crates/jellyflow-runtime/README.md,tests]
  Goal: Keep XyFlow compatibility projections aligned with canonical policy terms and document migration guidance for policy-related fields.
  Validation: `cargo nextest run -p jellyflow-runtime runtime`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: compatibility tests and README wording.
  Handoff: DONE 2026-05-30. README and XyFlow module docs now point canonical effective interaction policy resolution to `runtime::policy` while keeping XyFlow naming inside `runtime::xyflow`.

- [x] JPB-060 [owner=codex] [deps=JPB-040,JPB-050] [scope=docs/workstreams/jellyflow-model-policy-boundary-v1]
  Goal: Close the lane with fresh gates and split any schema migration or geometry extraction follow-ons.
  Validation: `cargo fmt --check`; `cargo nextest run --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `python3 tools/check_no_fret_dependencies.py`; `python3 tools/check_external_consumer_smoke.py`.
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: EVIDENCE_AND_GATES.md, HANDOFF.md, closeout audit.
  Handoff: DONE 2026-05-30. Final gates passed and the lane is closed. Delete planner ownership and persisted schema migration are split follow-ons.
