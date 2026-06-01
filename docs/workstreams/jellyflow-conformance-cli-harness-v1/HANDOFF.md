# Jellyflow Conformance CLI Harness v1 - Handoff

Status: Closed
Last updated: 2026-06-01

## Current State

The workstream is closed as a follow-on to golden approval APIs. Existing public APIs can run and
approve fixture directories; this lane added a small example command for agent and CI use.

JCCH-010 is complete: workstream docs, task ledger, campaign record, gates, and context manifest are
created.

JCCH-020 is complete: `crates/jellyflow-runtime/examples/conformance_harness.rs` supports
`check <fixture-dir>` and `approve <fixture-dir>`, emits pretty JSON reports, returns non-zero for
check mismatches/errors and approval errors, and has example-local tests.

JCCH-030 is complete: README/runtime README document check and approve commands, closeout evidence
is recorded, and the workstream is closed.

## Next Task

None in this workstream. Follow-ons are split below.

## Decisions Since Opening

- Keep the harness in `jellyflow-runtime` examples instead of adding a new crate.
- Use only standard-library argument parsing.
- Emit pretty JSON reports to stdout.
- Return non-zero for check mismatches/errors and approve errors.

## Validation To Run

- `cargo nextest run -p jellyflow-runtime --example conformance_harness`
- `cargo check -p jellyflow-runtime --examples`
- `cargo fmt --check`
- `cargo nextest run -p jellyflow-runtime`
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
- `jq empty docs/workstreams/jellyflow-conformance-cli-harness-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-cli-harness-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-cli-harness-v1/CAMPAIGNS.jsonl`

## Evidence So Far

- 2026-06-01: JCCH-010 opened the workstream.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime --example conformance_harness` passed,
  3 tests. Run ID: `a971fa87-eeab-4da4-bfde-c412905601e8`.
- 2026-06-01: `cargo check -p jellyflow-runtime --examples` passed.
- 2026-06-01: `cargo fmt --check` passed at closeout.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime` passed, 177 tests run at closeout.
  Run ID: `3852c6ee-7004-45a9-a9e1-4f217ae33f7b`.
- 2026-06-01: `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings` passed at
  closeout.

## Follow-On Candidates

- Dedicated `jellyflow-cli` crate if more commands accumulate.
- Renderer screenshot or pixel fixture assets outside `jellyflow-runtime`.
