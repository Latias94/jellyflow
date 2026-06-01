# Jellyflow Conformance CLI Harness v1 - Evidence And Gates

Status: Closed
Last updated: 2026-06-01

## Smallest Current Repro

Agents can call runtime approval APIs in Rust, but cannot run a fixture directory through a simple
check/approve command entry point.

## Required Gates

- `cargo nextest run -p jellyflow-runtime --example conformance_harness`
- `cargo check -p jellyflow-runtime --examples`
- `cargo fmt --check`
- `cargo nextest run -p jellyflow-runtime`
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
- `jq empty docs/workstreams/jellyflow-conformance-cli-harness-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-cli-harness-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-cli-harness-v1/CAMPAIGNS.jsonl`
- `git diff --check`

## Evidence Log

- 2026-06-01: JCCH-010 opened the conformance CLI harness lane.
  - Scope is limited to a renderer-free example command for check/approve fixture directories.
  - New CLI crate, parser dependencies, renderer assets, screenshots, and pixels remain out of
    scope.
- 2026-06-01: JCCH-020 added the renderer-free conformance harness example.
  - Added `crates/jellyflow-runtime/examples/conformance_harness.rs`.
  - `check <fixture-dir>` prints a pretty JSON directory report and exits non-zero for mismatches or
    execution errors.
  - `approve <fixture-dir>` explicitly writes approved actual traces to JSON and prints a pretty
    JSON approval report.
  - Example-local tests cover stale check failure, approve write-back followed by passing check, and
    usage errors.
  - `cargo nextest run -p jellyflow-runtime --example conformance_harness`: passed, 3 tests.
    - Nextest run ID: `a971fa87-eeab-4da4-bfde-c412905601e8`.
  - `cargo check -p jellyflow-runtime --examples`: passed.
- 2026-06-01: JCCH-030 closed the conformance CLI harness workstream.
  - README/runtime README document check and approve commands.
  - Closeout audit records review, verification, and follow-ons.
  - `cargo fmt --check`: passed.
  - `cargo nextest run -p jellyflow-runtime`: 177 passed, 0 skipped.
    - Nextest run ID: `3852c6ee-7004-45a9-a9e1-4f217ae33f7b`.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `jq empty docs/workstreams/jellyflow-conformance-cli-harness-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-cli-harness-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-cli-harness-v1/CAMPAIGNS.jsonl`: passed.
  - `git diff --check`: passed.

## Notes

This workstream is closed. Future screenshot, pixel, GPU, and adapter-smoke harnesses belong
outside `jellyflow-runtime` per ADR 0003.
