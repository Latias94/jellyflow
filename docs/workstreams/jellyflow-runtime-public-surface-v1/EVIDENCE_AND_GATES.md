# Jellyflow Runtime Public Surface v1 - Evidence And Gates

Status: Closed
Last updated: 2026-05-30

## Smallest Current Repro

```bash
cargo check -p jellyflow-runtime
```

This proves the runtime public surface and internal imports remain coherent after each slice.

## Gate Set

### Targeted Iteration Gates

```bash
cargo check -p jellyflow-runtime
cargo nextest run -p jellyflow-runtime
cargo nextest run -p jellyflow-runtime runtime
cargo nextest run -p jellyflow-runtime io
```

### External Consumer Gate

```bash
python3 tools/check_external_consumer_smoke.py
```

This proves an outside project can still path-depend on Jellyflow without pulling Fret packages.

### Dependency Boundary Gate

```bash
python3 tools/check_no_fret_dependencies.py
```

This proves `jellyflow-core` and `jellyflow-runtime` stay free of Fret packages.

### Broader Closeout Gate

```bash
cargo fmt --check
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Use a narrower closeout gate only if the workspace becomes too slow or an unrelated failure is
recorded with evidence.

### Review Gate

Run `review-workstream` before accepting task or lane completion. Record blocking findings, missing
gates, and residual risks here or link to the review note.

## Evidence Anchors

- `docs/workstreams/jellyflow-runtime-public-surface-v1/DESIGN.md`
- `docs/workstreams/jellyflow-runtime-public-surface-v1/TODO.md`
- `docs/workstreams/jellyflow-runtime-public-surface-v1/MILESTONES.md`
- `crates/jellyflow-runtime/src/lib.rs`
- `crates/jellyflow-runtime/src/io/`
- `crates/jellyflow-runtime/src/runtime/`
- `README.md`
- `crates/jellyflow-runtime/README.md`

## Fresh Evidence Log

- 2026-05-30: JRP-010 opened the lane and froze the task ledger. No code gates run yet.
- 2026-05-30: JRP-020 removed `jellyflow-runtime` pass-through `core`, `interaction`, `ops`, and `types` modules and updated internal imports to `jellyflow_core`.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo nextest run -p jellyflow-runtime`: passed with 67 tests.
  - `python3 tools/check_external_consumer_smoke.py`: passed; external cargo tree contained no `fret` or `fret-*` packages.
- 2026-05-30: JRP-030 moved XyFlow-compatible changes, apply helpers, and callback adapters under `runtime::xyflow`.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo nextest run -p jellyflow-runtime runtime`: passed with 48 tests selected and 19 skipped.
- 2026-05-30: JRP-040 split `io/mod.rs` into `config`, `files`, `tuning`, `view_state`, and `tests` modules, and removed the `.fret` default editor-state path helper.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo nextest run -p jellyflow-runtime io`: passed with 31 tests selected and 36 skipped.
  - `python3 tools/check_external_consumer_smoke.py`: passed; external cargo tree contained no `fret` or `fret-*` packages.
  - `rg -n "default_project_editor_state_path|\\.fret" crates/jellyflow-runtime/src/io crates/jellyflow-runtime/README.md README.md`: no matches.
- 2026-05-30: JRP-050 split `NodeGraphStore` internals into private `dispatch`, `events`, `subscriptions`, and `view` submodules while keeping `runtime::store::NodeGraphStore` as the public facade.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo nextest run -p jellyflow-runtime runtime`: passed with 48 tests selected and 19 skipped.
  - Post-format line counts: `store.rs` 179, `store/dispatch.rs` 285, `store/events.rs` 56, `store/subscriptions.rs` 143, `store/view.rs` 291.
- 2026-05-30: JRP-060 closeout updated public README wording and recorded final review/verification evidence.
  - `rg -n "runtime::(changes|apply|callbacks)|jellyflow_runtime::(core|interaction|ops|types)|default_project_editor_state_path|\\.fret" --glob '!docs/workstreams/**' --glob '!repo-ref/**'`: no matches.
  - `cargo fmt --check`: passed.
  - `cargo nextest run --workspace`: passed with 115 tests.
  - `cargo clippy --workspace --all-targets -- -D warnings`: passed.
  - `python3 tools/check_no_fret_dependencies.py`: passed; `jellyflow-core` and `jellyflow-runtime` had no `fret` or `fret-*` packages within depth 2.
  - `python3 tools/check_external_consumer_smoke.py`: passed; external temp consumer checked and cargo tree contained no `fret` or `fret-*` packages.
  - `jq empty docs/workstreams/jellyflow-runtime-public-surface-v1/WORKSTREAM.json`: passed after closeout edits.
  - `git diff --check`: passed after closeout edits.

## Review And Verification

- 2026-05-30: `review-workstream` self-review found no blocking findings.
  - Workstream compliance: JRP-020 through JRP-060 are complete; scope stayed within runtime public-surface cleanup; ADR 0331 dependency boundary is preserved.
  - Code quality: compatibility code is explicit under `runtime::xyflow`; IO and store internals are private focused modules; no public store trait proliferation was introduced.
  - Missing gates: none after the JRP-060 final gate set.
  - Residual risk: public API breakage is intentional before crates.io publish; downstream callers must migrate to `jellyflow_core` imports and `runtime::xyflow` compatibility paths.
- 2026-05-30: `verify-rust-workstream` closeout claim verified with fresh command evidence listed above.

## Notes

Fresh verification is required before marking a task, Codex goal, or lane complete.
