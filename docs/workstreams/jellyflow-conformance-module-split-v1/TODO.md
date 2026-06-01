# Jellyflow Conformance Module Split v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JCMS-010 [owner=codex] [scope=docs/workstreams/jellyflow-conformance-module-split-v1]
  Goal: Open the conformance module split workstream from the fearless-refactor audit.
  Validation: `jq empty docs/workstreams/jellyflow-conformance-module-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-module-split-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-module-split-v1/CAMPAIGNS.jsonl`
  Review: planner self-review for artifact agreement.
  Evidence: Workstream docs, task ledger, context manifest, and gates are created.

## M1 - Module Split

- [x] JCMS-020 [owner=codex] [deps=JCMS-010] [scope=crates/jellyflow-runtime/src/runtime/conformance]
  Goal: Split `runtime::conformance` into focused submodules without changing public API or behavior.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime conformance`; `cargo nextest run -p jellyflow-runtime --example conformance_harness`; `cargo nextest run -p jellyflow-runtime --test public_surface`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
  Review: review-workstream before accepting completion.
  Evidence: `mod.rs` is a facade and conformance tests still pass.
  Handoff: DONE 2026-06-01. Split `runtime::conformance` into `scenario`, `runner`,
  `reports`, `fixtures`, and `approval` modules while preserving public re-exports from `mod.rs`.

## M2 - Documentation And Closeout

- [x] JCMS-030 [owner=codex] [deps=JCMS-020] [scope=docs/workstreams/jellyflow-conformance-module-split-v1]
  Goal: Record final evidence, check for stale task state, and close the lane or split follow-ons.
  Validation: `jq empty docs/workstreams/jellyflow-conformance-module-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-module-split-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-module-split-v1/CAMPAIGNS.jsonl`; `git diff --check`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: Closeout audit records the behavior-preserving split and validation.
  Handoff: DONE 2026-06-01. Closeout audit records review and verification; the lane is closed
  with no follow-ons needed for this refactor.
