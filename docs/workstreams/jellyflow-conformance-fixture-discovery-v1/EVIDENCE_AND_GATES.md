# Jellyflow Conformance Fixture Discovery v1 - Evidence And Gates

Status: Closed
Last updated: 2026-06-01

## Smallest Current Repro

Adapters can load individual `ConformanceSuite` JSON files, but cannot ask the runtime conformance
API to discover all suite fixtures under a directory in deterministic order.

## Required Gates

- `cargo nextest run -p jellyflow-runtime conformance_fixture_directory`
- `cargo nextest run -p jellyflow-runtime --test public_surface`
- `cargo check -p jellyflow-runtime`
- `cargo fmt --check`
- `cargo nextest run -p jellyflow-runtime`
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
- `jq empty docs/workstreams/jellyflow-conformance-fixture-discovery-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-fixture-discovery-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-fixture-discovery-v1/CAMPAIGNS.jsonl`
- `git diff --check`

## Evidence Log

- 2026-06-01: JCFD-010 opened the fixture discovery lane.
  - Scope is limited to deterministic JSON suite file discovery and path-aware aggregate reports.
  - Golden approval/update workflows and renderer golden assets remain out of scope.
- 2026-06-01: JCFD-020 added deterministic fixture directory discovery.
  - Added `ConformanceFixtureDirectory`, `ConformanceSuiteFile`, `ConformanceSuiteFileReport`, and
    `ConformanceFixtureDirectoryReport`.
  - Discovery walks directories recursively, loads `*.json` suites in sorted path order, and keeps
    source paths attached to suites and reports.
  - Directory fixture and report types derive serde traits for agent-readable aggregate output.
  - Tests cover recursive sorted discovery, optional missing directories, and invalid JSON path
    context.
  - Public-surface smoke coverage uses the directory loader and report types.
  - `cargo nextest run -p jellyflow-runtime conformance_fixture_directory`: passed, 3 tests.
    - Nextest run ID: `b8fd5eb5-cc74-456f-92ee-2ae877ee6a35`.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 3 tests.
    - Nextest run ID: `02f40c26-70bf-4d41-a0c0-c91888787616`.
  - `cargo check -p jellyflow-runtime`: passed.
- 2026-06-01: JCFD-030 closed the fixture discovery workstream.
  - `cargo fmt --check`: passed.
  - `cargo nextest run -p jellyflow-runtime`: 173 passed, 0 skipped.
    - Nextest run ID: `3de7f38c-d1b1-417e-803e-fc156031d79d`.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 3 tests.
    - Nextest run ID: `02f40c26-70bf-4d41-a0c0-c91888787616`.
  - `cargo check -p jellyflow-runtime`: passed.
  - `jq empty docs/workstreams/jellyflow-conformance-fixture-discovery-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-fixture-discovery-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-fixture-discovery-v1/CAMPAIGNS.jsonl`: passed.
  - `git diff --check`: passed.
  - Documentation: `README.md`, `crates/jellyflow-runtime/README.md`, and
    `CLOSEOUT_AUDIT_2026-06-01.md`.

## Notes

This workstream is closed. Follow-ons are split below in `HANDOFF.md` and the closeout audit.
