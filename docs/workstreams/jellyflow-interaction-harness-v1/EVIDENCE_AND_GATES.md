# Jellyflow Interaction Harness v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-01

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime adapter_conformance
```

This exercises existing adapter conformance scenarios that are currently handwritten rather than
driven by a shared harness.

## Gate Set

### Harness Tracer Gate

```bash
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo check -p jellyflow-runtime
```

This proves the harness can validate observable runtime behavior without renderer dependencies.

### Runtime Package Gate

```bash
cargo nextest run -p jellyflow-runtime
```

This proves the harness and fixture changes do not regress runtime behavior.

### Broader Closeout Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
git diff --check
```

This proves formatting, runtime behavior, lint cleanliness, and diff hygiene.

## Evidence Anchors

- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `docs/workstreams/jellyflow-geometry-spatial-v1/HANDOFF.md`
- `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance.rs`
- `crates/jellyflow-runtime/src/runtime/tests/fixtures.rs`
- `crates/jellyflow-runtime/src/runtime/store.rs`
- `crates/jellyflow-runtime/src/runtime/events/`

## Fresh Evidence Log

- 2026-06-01: JIH-010 opened the interaction harness workstream.
  - `git status --short --branch`: clean before opening docs, branch ahead of origin.
  - Governing decision: ADR 0003 keeps harness and adapter conformance renderer-free.
- 2026-06-01: JIH-020 added the private runtime interaction harness tracer bullet.
  - Added `crates/jellyflow-runtime/src/runtime/tests/harness.rs` with normalized graph commit, view, and gesture trace events.
  - Registered the harness in `crates/jellyflow-runtime/src/runtime/tests.rs`.
  - Migrated adapter conformance connect, reconnect, delete, viewport/selection, and gesture lifecycle checks to scenario-aware trace assertions.
  - RED gate: `cargo nextest run -p jellyflow-runtime adapter_conformance` failed before the harness module existed.
  - `cargo fmt`: applied formatting after `cargo fmt --check` reported style-only differences.
  - `cargo fmt --check`: passed after formatting.
  - `cargo nextest run -p jellyflow-runtime adapter_conformance`: passed, 6 tests.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo nextest run -p jellyflow-runtime`: passed, 139 tests.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `jq empty docs/workstreams/jellyflow-interaction-harness-v1/WORKSTREAM.json`: passed.
  - `git diff --check`: passed.
- 2026-06-01: JIH-030 added the renderer-neutral selection-box fixture and helper.
  - Added `crates/jellyflow-runtime/src/runtime/selection.rs`.
  - Exposed `runtime::selection` from `crates/jellyflow-runtime/src/runtime/mod.rs`.
  - Added harness-backed selection tests for replacement and additive selection.
  - Fixture coverage: hidden nodes are skipped; `selectable=false` nodes are skipped; non-selectable edges are skipped; default connected-edge selection includes edges connected to selected nodes; result nodes/edges are sorted; selection events are asserted through the harness.
  - RED gate: `cargo nextest run -p jellyflow-runtime selection` failed before `runtime::selection` and `InteractionHarness::store_mut` existed.
  - `cargo fmt`: applied formatting after `cargo fmt --check` reported style-only differences.
  - `cargo fmt --check`: passed after formatting.
  - `cargo nextest run -p jellyflow-runtime selection`: passed, 6 tests.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 2 tests.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo nextest run -p jellyflow-runtime`: passed, 141 tests.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `jq empty docs/workstreams/jellyflow-interaction-harness-v1/WORKSTREAM.json`: passed.
  - `git diff --check`: passed.

## Notes

- Fresh verification is required before marking a task, Codex goal, or lane complete.
- Keep the first harness private to runtime tests until multiple interaction fixtures prove the
  fixture language.
