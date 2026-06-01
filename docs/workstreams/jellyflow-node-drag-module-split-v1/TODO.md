# Jellyflow Node Drag Module Split v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JNDMS-010 [owner=codex] [scope=docs/workstreams/jellyflow-node-drag-module-split-v1]
  Goal: Open the node drag module split workstream from the fearless-refactor audit.
  Validation: `jq empty docs/workstreams/jellyflow-node-drag-module-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-drag-module-split-v1/TASKS.jsonl docs/workstreams/jellyflow-node-drag-module-split-v1/CAMPAIGNS.jsonl`
  Review: planner self-review for artifact agreement.
  Evidence: Workstream docs, task ledger, context manifest, and gates are created.

## M1 - Module Split

- [x] JNDMS-020 [owner=codex] [deps=JNDMS-010] [scope=crates/jellyflow-runtime/src/runtime/drag]
  Goal: Split `runtime::drag` into focused submodules without changing public API or behavior.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime drag`; `cargo nextest run -p jellyflow-runtime conformance`; `cargo nextest run -p jellyflow-runtime --test public_surface`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
  Review: review-workstream before accepting completion.
  Evidence: `runtime/drag/mod.rs` is a facade over `types`, `planner`, `candidates`,
  `constraints`, and `store`; drag, conformance, public-surface, package, and clippy gates pass.

## M2 - Documentation And Closeout

- [x] JNDMS-030 [owner=codex] [deps=JNDMS-020] [scope=docs/workstreams/jellyflow-node-drag-module-split-v1]
  Goal: Record final evidence, check for stale task state, and close the lane or split follow-ons.
  Validation: `jq empty docs/workstreams/jellyflow-node-drag-module-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-drag-module-split-v1/TASKS.jsonl docs/workstreams/jellyflow-node-drag-module-split-v1/CAMPAIGNS.jsonl`; `git diff --check`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: Closeout audit records the behavior-preserving split and validation; no follow-on is
  required for this narrow module-boundary lane.
