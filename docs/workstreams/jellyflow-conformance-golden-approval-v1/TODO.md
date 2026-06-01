# Jellyflow Conformance Golden Approval v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JCGA-010 [owner=codex] [scope=docs/workstreams/jellyflow-conformance-golden-approval-v1]
  Goal: Open the golden approval workstream from fixture discovery follow-ons.
  Validation: `jq empty docs/workstreams/jellyflow-conformance-golden-approval-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-golden-approval-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-golden-approval-v1/CAMPAIGNS.jsonl`
  Review: planner self-review for artifact agreement.
  Evidence: Workstream docs, task ledger, context manifest, and gates are created.

## M1 - Approval Update API

- [x] JCGA-020 [owner=codex] [deps=JCGA-010] [scope=crates/jellyflow-runtime/src/runtime/conformance/mod.rs,crates/jellyflow-runtime/src/runtime/tests/conformance.rs,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Add headless approval/update primitives for suite, file, and directory fixtures.
  Validation: `cargo nextest run -p jellyflow-runtime conformance_approval`; `cargo nextest run -p jellyflow-runtime --test public_surface`; `cargo check -p jellyflow-runtime`
  Review: review-workstream before accepting completion.
  Evidence: Approval/update tests and public-surface smoke coverage.
  Handoff: DONE 2026-06-01. Added suite approval reports, explicit file/directory write-back,
  approval execution errors, focused tests, and public-surface coverage.

## M2 - Documentation And Closeout

- [x] JCGA-030 [owner=codex] [deps=JCGA-020] [scope=README.md,crates/jellyflow-runtime/README.md,docs/workstreams/jellyflow-conformance-golden-approval-v1]
  Goal: Document headless approval/update workflow, record fresh evidence, and close the lane or
  split follow-ons.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`; `jq empty docs/workstreams/jellyflow-conformance-golden-approval-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-golden-approval-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-golden-approval-v1/CAMPAIGNS.jsonl`; `git diff --check`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: README/runtime README explain explicit approval write-back before renderer tests.
  Handoff: DONE 2026-06-01. Updated README/runtime README, recorded closeout audit and final
  verification evidence, closed the workstream, and split CLI harness plus renderer golden assets as
  follow-ons.
