# Jellyflow Conformance Schema Runner Split v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JCSR-010 [owner=codex] [scope=docs/workstreams/jellyflow-conformance-schema-runner-split-v1]
  Goal: Open the conformance schema/runner split workstream from the fearless-refactor audit.
  Validation: `jq empty docs/workstreams/jellyflow-conformance-schema-runner-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-schema-runner-split-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-schema-runner-split-v1/CAMPAIGNS.jsonl`
  Review: planner self-review for artifact agreement.
  Evidence: Workstream docs, task ledger, context manifest, and gates are created.

## M1 - Scenario Schema Split

- [x] JCSR-020 [owner=codex] [deps=JCSR-010] [scope=crates/jellyflow-runtime/src/runtime/conformance/scenario]
  Goal: Split conformance scenario schema into focused private submodules without changing public
  API paths or fixture JSON shape.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime conformance`; `cargo nextest run -p jellyflow-runtime --test public_surface`
  Review: review-workstream before accepting completion.
  Evidence: `scenario/mod.rs` is a facade over `constants`, `suite`, `setup`, `action`, and
  `trace`; conformance and public-surface tests pass.

## M2 - Runner Split

- [x] JCSR-030 [owner=codex] [deps=JCSR-020] [scope=crates/jellyflow-runtime/src/runtime/conformance/runner]
  Goal: Split conformance runner action execution, store tracing, and callback tracing into focused
  private submodules without changing behavior.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime conformance`; `cargo nextest run -p jellyflow-runtime --example conformance_harness`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
  Review: review-workstream before accepting completion.
  Evidence: `runner/mod.rs` is a facade over `actions`, `trace`, and `callbacks`; conformance,
  example harness, package, and clippy gates pass.

## M3 - Documentation And Closeout

- [x] JCSR-040 [owner=codex] [deps=JCSR-030] [scope=docs/workstreams/jellyflow-conformance-schema-runner-split-v1]
  Goal: Record final evidence, scan for stale task state, and close the lane or split follow-ons.
  Validation: `jq empty docs/workstreams/jellyflow-conformance-schema-runner-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-schema-runner-split-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-schema-runner-split-v1/CAMPAIGNS.jsonl`; `git diff --check`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: Closeout audit records the behavior-preserving schema/runner split and validation; no
  follow-on is required for this narrow private-boundary lane.
