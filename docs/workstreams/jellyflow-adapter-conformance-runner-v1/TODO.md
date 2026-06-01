# Jellyflow Adapter Conformance Runner v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JACR-010 [owner=codex] [scope=docs/workstreams/jellyflow-adapter-conformance-runner-v1]
  Goal: Open the adapter conformance runner workstream from conformance and auto-pan follow-ons.
  Validation: `jq empty docs/workstreams/jellyflow-adapter-conformance-runner-v1/WORKSTREAM.json docs/workstreams/jellyflow-adapter-conformance-runner-v1/TASKS.jsonl docs/workstreams/jellyflow-adapter-conformance-runner-v1/CAMPAIGNS.jsonl`
  Review: planner self-review for artifact agreement.
  Evidence: Workstream docs, task ledger, context manifest, and gates are created.

## M1 - Suite Runner

- [x] JACR-020 [owner=codex] [deps=JACR-010] [scope=crates/jellyflow-runtime/src/runtime/conformance/mod.rs,crates/jellyflow-runtime/src/runtime/tests/conformance.rs,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Add a public conformance suite runner and aggregate report for adapter crates.
  Validation: `cargo nextest run -p jellyflow-runtime conformance_suite`; `cargo nextest run -p jellyflow-runtime --test public_surface`; `cargo check -p jellyflow-runtime`
  Review: review-workstream before accepting completion.
  Evidence: Suite runner tests and public-surface smoke coverage.
  Handoff: DONE 2026-06-01. Added `ConformanceSuite`, `ConformanceSuiteReport`,
  `run_conformance_suite`, tests for mismatch/error aggregation, and public-surface coverage.

## M2 - Documentation And Closeout

- [x] JACR-030 [owner=codex] [deps=JACR-020] [scope=README.md,crates/jellyflow-runtime/README.md,docs/workstreams/jellyflow-adapter-conformance-runner-v1]
  Goal: Document suite runner usage, record fresh evidence, and close the lane or split follow-ons.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`; `jq empty docs/workstreams/jellyflow-adapter-conformance-runner-v1/WORKSTREAM.json`; `git diff --check`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: Runtime README explains scenario suites before renderer smoke tests.
  Handoff: DONE 2026-06-01. Updated README/runtime README, recorded closeout audit and final
  verification evidence, closed the workstream, and split follow-ons for file-backed fixture
  loading, adapter crate templates, and renderer smoke-test helpers.
