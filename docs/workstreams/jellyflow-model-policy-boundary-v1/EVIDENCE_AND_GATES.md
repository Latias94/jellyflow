# Jellyflow Model Policy Boundary v1 - Evidence And Gates

Status: Closed
Last updated: 2026-05-30

## Smallest Current Repro

```bash
rg -n "selectable|draggable|connectable|deletable|reconnectable|extent|expand_parent|hidden|collapsed" crates/jellyflow-core/src crates/jellyflow-runtime/src
```

This shows the current policy/layout/presentation fields that need a clearer ownership contract.

## Gate Set

### Targeted Iteration Gates

```bash
cargo check -p jellyflow-runtime
cargo nextest run -p jellyflow-runtime policy
cargo nextest run -p jellyflow-runtime rules
cargo nextest run -p jellyflow-runtime runtime
```

### Dependency Boundary Gate

```bash
python3 tools/check_no_fret_dependencies.py
```

### External Consumer Gate

```bash
python3 tools/check_external_consumer_smoke.py
```

### Broader Closeout Gate

```bash
cargo fmt --check
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

### Review Gate

Run `review-workstream` before accepting task or lane completion. Run `verify-rust-workstream`
before marking the lane complete.

## Evidence Anchors

- `docs/adr/0331-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/workstreams/jellyflow-runtime-public-surface-v1/CLOSEOUT_AUDIT_2026-05-30.md`
- `crates/jellyflow-core/src/core/model.rs`
- `crates/jellyflow-core/src/ops/mod.rs`
- `crates/jellyflow-runtime/src/io/config.rs`
- `crates/jellyflow-runtime/src/rules/mod.rs`
- `crates/jellyflow-runtime/src/runtime/xyflow/changes.rs`

## Fresh Evidence Log

- 2026-05-30: JPB-010 opened the follow-on lane and froze the task ledger. No code gates run yet.
- 2026-05-30: JPB-020 classified existing model/config fields and recorded the additive v1 decision.
  - `docs/adr/0332-jellyflow-model-policy-boundary.md`: added.
  - `docs/workstreams/jellyflow-model-policy-boundary-v1/JPB-020_FIELD_TAXONOMY_2026-05-30.md`: added.
  - `cargo fmt --check`: passed.
  - `git diff --check`: passed.
  - `jq empty docs/workstreams/jellyflow-model-policy-boundary-v1/WORKSTREAM.json`: passed.
  - `review-workstream` self-review: no blocking findings; JPB-020 stayed in docs/decision scope and does not move persisted fields.
  - `verify-rust-workstream` claim: JPB-020 decision record exists and task-local formatting/diff/JSON gates passed.
- 2026-05-30: JPB-030 added `jellyflow_runtime::runtime::policy` as the read-only effective interaction policy facade.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo nextest run -p jellyflow-runtime policy`: passed with 5 selected tests and 65 skipped.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `review-workstream` self-review: no blocking findings; APIs are pure/read-only and stay inside the assigned runtime scope.
  - `verify-rust-workstream` claim: JPB-030 policy facade exists and task-local compile/test/lint gates passed.
- 2026-05-30: JPB-040 routed connect and reconnect planners through `runtime::policy`.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo nextest run -p jellyflow-runtime rules`: passed with 13 selected tests and 59 skipped.
  - `cargo nextest run -p jellyflow-runtime runtime`: passed with 51 selected tests and 21 skipped.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `review-workstream` self-review: no blocking findings for connect/reconnect; delete enforcement remains a named follow-on because no delete planner exists.
  - `verify-rust-workstream` claim: JPB-040 connect/reconnect behavior is policy-backed and task-local compile/test/lint gates passed.
- 2026-05-30: JPB-050 updated public and compatibility docs for canonical policy terminology.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo nextest run -p jellyflow-runtime runtime`: passed with 51 selected tests and 21 skipped.
  - `review-workstream` self-review: no blocking findings; XyFlow naming remains isolated under `runtime::xyflow`.
  - `verify-rust-workstream` claim: JPB-050 docs/compatibility alignment is present and task-local compile/runtime gates passed.
- 2026-05-30: JPB-060 closed the lane and recorded final gates.
  - `cargo fmt --check`: passed.
  - `jq empty docs/workstreams/jellyflow-model-policy-boundary-v1/WORKSTREAM.json`: passed.
  - `git diff --check`: passed.
  - `cargo nextest run --workspace`: passed with 120 tests.
  - `cargo clippy --workspace --all-targets -- -D warnings`: passed.
  - `python3 tools/check_no_fret_dependencies.py`: passed; `jellyflow-core` and `jellyflow-runtime` had no `fret` or `fret-*` packages within depth 2.
  - `python3 tools/check_external_consumer_smoke.py`: passed; external temp consumer checked and cargo tree contained no `fret` or `fret-*` packages.
  - `review-workstream` self-review: no blocking findings; delete planner and schema migration are follow-ons.
  - `verify-rust-workstream` closeout claim: model/policy boundary v1 is implemented and final gates passed.

## Notes

- Fresh verification is required before marking a task, Codex goal, or lane complete.
- Do not treat a helper API as permission to move persisted fields; schema movement needs the
  JPB-020 decision record.
