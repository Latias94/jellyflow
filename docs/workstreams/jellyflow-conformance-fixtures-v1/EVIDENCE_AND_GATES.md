# Jellyflow Conformance Fixtures v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-01

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime adapter_conformance
```

This currently exercises the private interaction harness and normalized traces. JCF-030 should add
a dedicated `conformance` test filter.

## Gate Set

### Fixture Vocabulary Gate

```bash
cargo nextest run -p jellyflow-runtime --test public_surface
cargo check -p jellyflow-runtime
```

This proves the exported fixture vocabulary stays visible and renderer-free.

### Conformance Runner Gate

```bash
cargo nextest run -p jellyflow-runtime conformance
cargo check -p jellyflow-runtime
```

This proves fixture scenarios execute against the headless runtime.

### Adapter Conformance Gate

```bash
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo check -p jellyflow-runtime
```

This proves existing adapter-conformance behavior survives fixture conversion.

### Runtime Package Gate

```bash
cargo nextest run -p jellyflow-runtime
```

This proves fixture changes do not regress runtime behavior.

### Broader Closeout Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-conformance-fixtures-v1/WORKSTREAM.json
git diff --check
```

This proves formatting, runtime behavior, lint cleanliness, JSON validity, and diff hygiene.

## Evidence Anchors

- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `docs/workstreams/jellyflow-interaction-harness-v1/CLOSEOUT_AUDIT_2026-06-01.md`
- `docs/workstreams/jellyflow-node-drag-kernel-v1/CLOSEOUT_AUDIT_2026-06-01.md`
- `docs/workstreams/jellyflow-runtime-public-surface-v1/CLOSEOUT_AUDIT_2026-05-30.md`
- `crates/jellyflow-runtime/src/runtime/tests/harness.rs`
- `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance.rs`

## Fresh Evidence Log

- 2026-06-01: JCF-010 opened the conformance fixtures workstream.
  - `git status --short --branch`: clean before opening docs, branch ahead of origin.
  - Governing decision: ADR 0003 keeps fixture execution renderer-free.
  - Source evidence: interaction harness closeout and node drag kernel closeout.
- 2026-06-01: JCF-020 added the public headless fixture vocabulary.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: PASS, 3 tests.
  - `cargo check -p jellyflow-runtime`: PASS.
  - `cargo fmt --check`: PASS.
  - `git diff --check`: PASS.
  - Evidence: `runtime::conformance` exposes scenario/setup/action/trace types that serialize
    without renderer dependencies; public surface test round-trips a node drag fixture.
- 2026-06-01: JCF-030 added the headless fixture runner.
  - `cargo nextest run -p jellyflow-runtime conformance`: PASS, 11 tests.
  - `cargo check -p jellyflow-runtime`: PASS.
  - `cargo fmt --check`: PASS.
  - Evidence: runner executes a node drag fixture against `NodeGraphStore`, records gesture,
    graph-commit, and callback trace events, and reports compact trace mismatches.
- 2026-06-01: JCF-040 converted connect and node drag adapter-conformance traces to fixtures.
  - `cargo nextest run -p jellyflow-runtime adapter_conformance`: PASS, 8 tests.
  - `cargo nextest run -p jellyflow-runtime conformance`: PASS, 11 tests.
  - `cargo check -p jellyflow-runtime`: PASS.
  - Evidence: connect dispatch, connect gesture lifecycle, connect gesture transaction callbacks,
    and node drag gesture callbacks now assert through `run_conformance_scenario`.

## Notes

- Fresh verification is required before marking a task, Codex goal, or lane complete.
- Keep screenshot, pixel, GPU, windowing, DOM, and adapter-specific smoke tests outside
  `jellyflow-runtime`.
- Do not turn the fixture schema into a general scripting language; start with connect and drag.
