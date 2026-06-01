# Jellyflow Conformance CLI Harness v1 - Design

Status: Closed
Date: 2026-06-01

## Problem

Jellyflow now exposes headless APIs for loading, discovering, running, and approving conformance
fixtures, but agents and CI still need a simple command entry point to run fixture directories in
check or approve mode.

## Target State

- `jellyflow-runtime` provides a renderer-free example harness.
- `check <fixture-dir>` loads a fixture directory, prints a JSON report, and exits non-zero on
  mismatches or execution errors.
- `approve <fixture-dir>` explicitly writes approved actual traces back to JSON files and prints a
  JSON approval report.
- The harness uses only existing runtime conformance APIs and standard-library argument parsing.
- The harness is tested without spawning nested cargo processes.

Target state is complete as of 2026-06-01.

## Scope

- Add `crates/jellyflow-runtime/examples/conformance_harness.rs`.
- Add example-local tests for check/approve behavior.
- Update README/runtime README and close the lane.

## Non-Goals

- No new workspace crate.
- No clap/argh/pico-args dependency.
- No renderer, screenshot, pixel, GPU, windowing, or platform adapter code.
- No fixture schema changes.

## Architecture Direction

Keep the CLI thin:

1. parse only `<mode> <fixture-dir>`;
2. delegate check mode to `ConformanceFixtureDirectory::run`;
3. delegate approve mode to `ConformanceFixtureDirectory::approve_actual_traces_to_json`;
4. serialize reports with `serde_json::to_writer_pretty`;
5. expose tests through example-internal helper functions.
