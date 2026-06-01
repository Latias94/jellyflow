# Jellyflow Conformance CLI Harness v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JCCH-010 [owner=codex] [scope=docs/workstreams/jellyflow-conformance-cli-harness-v1]
  Goal: Open the CLI harness workstream from golden approval follow-ons.
  Validation: `jq empty docs/workstreams/jellyflow-conformance-cli-harness-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-cli-harness-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-cli-harness-v1/CAMPAIGNS.jsonl`
  Review: planner self-review for artifact agreement.
  Evidence: Workstream docs, task ledger, context manifest, and gates are created.

## M1 - Harness Example

- [x] JCCH-020 [owner=codex] [deps=JCCH-010] [scope=crates/jellyflow-runtime/examples/conformance_harness.rs]
  Goal: Add check/approve example harness for conformance fixture directories.
  Validation: `cargo nextest run -p jellyflow-runtime --example conformance_harness`; `cargo check -p jellyflow-runtime --examples`
  Review: review-workstream before accepting completion.
  Evidence: Example tests prove stale fixtures fail check, approve writes expected traces, and check passes afterward.
  Handoff: DONE 2026-06-01. Added `conformance_harness` with check/approve modes, pretty JSON
  output, usage errors, and example-local tests.

## M2 - Documentation And Closeout

- [x] JCCH-030 [owner=codex] [deps=JCCH-020] [scope=README.md,crates/jellyflow-runtime/README.md,docs/workstreams/jellyflow-conformance-cli-harness-v1]
  Goal: Document the example harness, record fresh evidence, and close the lane or split follow-ons.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`; `jq empty docs/workstreams/jellyflow-conformance-cli-harness-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-cli-harness-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-cli-harness-v1/CAMPAIGNS.jsonl`; `git diff --check`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: README/runtime README show check and approve commands.
  Handoff: DONE 2026-06-01. README/runtime README document the harness commands, fresh package
  gates are recorded, and the workstream is closed.
