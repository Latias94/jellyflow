# Jellyflow Adapter Conformance Runner v1 - Evidence And Gates

Status: Closed
Last updated: 2026-06-01

## Smallest Current Repro

Adapters can call `run_conformance_scenario`, but they have no public suite-level helper for running
multiple scenarios and collecting a single aggregate report.

## Required Gates

- `cargo nextest run -p jellyflow-runtime conformance_suite`
- `cargo nextest run -p jellyflow-runtime --test public_surface`
- `cargo check -p jellyflow-runtime`
- `cargo fmt --check`
- `cargo nextest run -p jellyflow-runtime`
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
- `jq empty docs/workstreams/jellyflow-adapter-conformance-runner-v1/WORKSTREAM.json docs/workstreams/jellyflow-adapter-conformance-runner-v1/TASKS.jsonl docs/workstreams/jellyflow-adapter-conformance-runner-v1/CAMPAIGNS.jsonl`
- `git diff --check`

## Evidence Log

- 2026-06-01: JACR-010 opened the adapter conformance runner lane.
  - Scope is limited to a public headless suite runner over existing conformance scenarios.
  - Renderer/platform runners, file-backed golden fixtures, and screenshot/pixel checks remain out
    of scope.
- 2026-06-01: JACR-020 added the public conformance suite runner.
  - Added `ConformanceSuite`, `ConformanceSuiteReport`, and `run_conformance_suite`.
  - Tests cover all-scenario execution, trace mismatch aggregation, execution error aggregation, and
    continuation after a scenario-level execution error.
  - Public-surface smoke coverage constructs, serializes, deserializes, and runs a suite.
  - `cargo nextest run -p jellyflow-runtime conformance_suite`: passed, 2 tests.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 3 tests.
  - `cargo check -p jellyflow-runtime`: passed.
- 2026-06-01: JACR-030 closed the adapter conformance runner workstream.
  - `cargo fmt --check`: passed.
  - `cargo nextest run -p jellyflow-runtime`: 167 passed, 0 skipped.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `jq empty docs/workstreams/jellyflow-adapter-conformance-runner-v1/WORKSTREAM.json docs/workstreams/jellyflow-adapter-conformance-runner-v1/TASKS.jsonl docs/workstreams/jellyflow-adapter-conformance-runner-v1/CAMPAIGNS.jsonl`: passed.
  - `git diff --check`: passed.
  - Documentation: `README.md`, `crates/jellyflow-runtime/README.md`, and
    `CLOSEOUT_AUDIT_2026-06-01.md`.

## Notes

This workstream is closed. Follow-ons are split below in `HANDOFF.md` and the closeout audit.
