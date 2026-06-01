# Jellyflow Conformance File Fixtures v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JCFF-010 [owner=codex] [scope=docs/workstreams/jellyflow-conformance-file-fixtures-v1]
  Goal: Open the conformance file fixtures workstream from suite runner follow-ons.
  Validation: `jq empty docs/workstreams/jellyflow-conformance-file-fixtures-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-file-fixtures-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-file-fixtures-v1/CAMPAIGNS.jsonl`
  Review: planner self-review for artifact agreement.
  Evidence: Workstream docs, task ledger, context manifest, and gates are created.

## M1 - File Loader

- [x] JCFF-020 [owner=codex] [deps=JCFF-010] [scope=crates/jellyflow-runtime/src/runtime/conformance/mod.rs,crates/jellyflow-runtime/src/runtime/tests/conformance.rs,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Add file-backed JSON load/save helpers for `ConformanceSuite`.
  Validation: `cargo nextest run -p jellyflow-runtime conformance_file`; `cargo nextest run -p jellyflow-runtime --test public_surface`; `cargo check -p jellyflow-runtime`
  Review: review-workstream before accepting completion.
  Evidence: File loader tests and public-surface smoke coverage.
  Handoff: DONE 2026-06-01. Added `ConformanceSuite::load_json`, `load_json_if_exists`,
  `save_json`, `ConformanceFixtureFileError`, focused file tests, and public-surface coverage.

## M2 - Documentation And Closeout

- [x] JCFF-030 [owner=codex] [deps=JCFF-020] [scope=README.md,crates/jellyflow-runtime/README.md,docs/workstreams/jellyflow-conformance-file-fixtures-v1]
  Goal: Document file-backed fixture suites, record fresh evidence, and close the lane or split
  follow-ons.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`; `jq empty docs/workstreams/jellyflow-conformance-file-fixtures-v1/WORKSTREAM.json`; `git diff --check`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: Runtime README explains JSON suite files before renderer smoke tests.
  Handoff: DONE 2026-06-01. Updated README/runtime README, recorded closeout audit and final
  verification evidence, closed the workstream, and split follow-ons for fixture discovery,
  approval workflow, and renderer golden assets outside runtime.
