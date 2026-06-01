# Jellyflow Edge Path Module Split v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JEPM-010 [owner=codex] [scope=docs/workstreams/jellyflow-edge-path-module-split-v1]
  Goal: Open the edge path module split workstream from the fearless-refactor audit.
  Validation: `jq empty docs/workstreams/jellyflow-edge-path-module-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-edge-path-module-split-v1/TASKS.jsonl docs/workstreams/jellyflow-edge-path-module-split-v1/CAMPAIGNS.jsonl`
  Review: planner self-review for artifact agreement.
  Evidence: Workstream docs, task ledger, context manifest, and gates are created.

## M1 - Module Split

- [x] JEPM-020 [owner=codex] [deps=JEPM-010] [scope=crates/jellyflow-runtime/src/runtime/geometry/paths]
  Goal: Split `runtime::geometry::paths` into focused submodules without changing public API or
  path behavior.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime geometry::paths`; `cargo nextest run -p jellyflow-runtime --test public_surface`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
  Review: review-workstream before accepting completion.
  Evidence: `paths/mod.rs` is a facade over `types`, `straight`, `bezier`, `smoothstep`, `label`,
  and `tests`; geometry path, public-surface, package, and clippy gates pass.

## M2 - Documentation And Closeout

- [x] JEPM-030 [owner=codex] [deps=JEPM-020] [scope=docs/workstreams/jellyflow-edge-path-module-split-v1]
  Goal: Record final evidence, scan for stale task state, and close the lane or split follow-ons.
  Validation: `jq empty docs/workstreams/jellyflow-edge-path-module-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-edge-path-module-split-v1/TASKS.jsonl docs/workstreams/jellyflow-edge-path-module-split-v1/CAMPAIGNS.jsonl`; `git diff --check`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: Closeout audit records the behavior-preserving split and validation; no follow-on is
  required for this narrow module-boundary lane.
