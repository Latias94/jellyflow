# Jellyflow Delete Contract v1 - TODO

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

- [x] JDC-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-delete-contract-v1,CONTEXT.md]
  Goal: Open the delete contract workstream from XyFlow delete source evidence, existing
  Jellyflow delete planner code, and stale model-policy follow-on language.
  Validation: jq empty docs/workstreams/jellyflow-delete-contract-v1/WORKSTREAM.json
  docs/workstreams/jellyflow-delete-contract-v1/TASKS.jsonl
  docs/workstreams/jellyflow-delete-contract-v1/CAMPAIGNS.jsonl
  docs/workstreams/jellyflow-delete-contract-v1/CONTEXT.jsonl; git diff --check
  Review: planner self-review for artifact agreement.
  Evidence: docs/workstreams/jellyflow-delete-contract-v1/DESIGN.md.
  Context: docs/workstreams/jellyflow-delete-contract-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Lane opened with JDC-020 as the first executable task.
  State: TASKS.jsonl entry JDC-010 matches this task.

## M1 - Template Delete Smoke

- [ ] JDC-020 [owner=codex] [deps=JDC-010] [scope=templates/headless-adapter,crates/jellyflow-runtime/src/runtime/tests]
  Goal: Add a template adapter delete selection scenario that uses `apply_delete_selection_for_key`
  and proves commit, XyFlow-style delete callbacks, and selection cleanup trace ordering.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime conformance; cargo
  nextest run -p jellyflow-runtime adapter_conformance; cargo test --manifest-path
  templates/headless-adapter/Cargo.toml; cargo run --manifest-path templates/headless-adapter/Cargo.toml
  -- check
  Review: review-workstream before accepting completion.
  Evidence: `templates/headless-adapter/src/lib.rs`,
  `crates/jellyflow-runtime/src/runtime/tests/conformance/runner/scenario.rs`, and
  `docs/workstreams/jellyflow-delete-contract-v1/EVIDENCE_AND_GATES.md`.
  Context: docs/workstreams/jellyflow-delete-contract-v1/CONTEXT.jsonl.
  Handoff: TODO.
  State: TASKS.jsonl entry JDC-020 must record DONE with validation and evidence.

## M2 - Documentation And Closeout

- [ ] JDC-030 [owner=codex] [deps=JDC-020] [scope=README.md,crates/jellyflow-runtime/README.md,CONTEXT.md,docs/workstreams/jellyflow-delete-contract-v1]
  Goal: Document delete runtime/adapter boundaries, clear stale follow-on navigation, record fresh
  evidence, and close or split follow-ons.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime; cargo clippy -p
  jellyflow-runtime --all-targets -- -D warnings; jq empty
  docs/workstreams/jellyflow-delete-contract-v1/WORKSTREAM.json
  docs/workstreams/jellyflow-delete-contract-v1/TASKS.jsonl
  docs/workstreams/jellyflow-delete-contract-v1/CAMPAIGNS.jsonl
  docs/workstreams/jellyflow-delete-contract-v1/CONTEXT.jsonl; git diff --check
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: README.md, crates/jellyflow-runtime/README.md, CONTEXT.md,
  EVIDENCE_AND_GATES.md, WORKSTREAM.json, and closeout audit.
  Context: docs/workstreams/jellyflow-delete-contract-v1/CONTEXT.jsonl.
  Handoff: TODO.
  State: TASKS.jsonl entry JDC-030 must record closeout validation, evidence, and handoff status.
