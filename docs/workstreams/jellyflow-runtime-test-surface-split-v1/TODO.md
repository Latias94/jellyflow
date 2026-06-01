# Jellyflow Runtime Test Surface Split v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JRTSS-010 [owner=codex] [scope=docs/workstreams/jellyflow-runtime-test-surface-split-v1]
  Goal: Open the runtime test-surface split workstream from the fearless-refactor audit.
  Validation: `jq empty docs/workstreams/jellyflow-runtime-test-surface-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-runtime-test-surface-split-v1/TASKS.jsonl docs/workstreams/jellyflow-runtime-test-surface-split-v1/CAMPAIGNS.jsonl`
  Review: planner self-review for artifact agreement.
  Evidence: Workstream docs, task ledger, context manifest, and gates are created.
  State: TASKS.jsonl entry JRTSS-010 matches this task.

## M1 - Conformance And Adapter Test Surface

- [x] JRTSS-020 [owner=codex] [deps=JRTSS-010] [scope=crates/jellyflow-runtime/src/runtime/tests]
  Goal: Split conformance, adapter-conformance, and harness test organization into focused
  behavior groups without changing test semantics, public API paths, fixture JSON shape, or trace
  expectations.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime conformance`; `cargo nextest run -p jellyflow-runtime --example conformance_harness`; `cargo nextest run -p jellyflow-runtime --test public_surface`
  Review: review-workstream before accepting completion.
  Evidence: `conformance`, `adapter_conformance`, and `harness` are split into focused test-only
  submodules; conformance, example harness, public-surface, package, and clippy gates pass.
  Context: docs/workstreams/jellyflow-runtime-test-surface-split-v1/CONTEXT.jsonl
  Handoff: DONE 2026-06-01. Test-only split completed without production runtime behavior,
  public API, fixture schema, conformance trace, or renderer-boundary changes.
  State: TASKS.jsonl entry JRTSS-020 records owner, scope, validation, evidence, and handoff status.

## M2 - Drag And Runtime Interaction Test Surface

- [x] JRTSS-030 [owner=codex] [deps=JRTSS-020] [scope=crates/jellyflow-runtime/src/runtime/tests]
  Goal: Split drag and interaction-heavy runtime tests into scenario families and local helpers
  without changing node drag, selection, viewport, auto-pan, store dispatch, or callback behavior.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime drag`; `cargo nextest run -p jellyflow-runtime selection`; `cargo nextest run -p jellyflow-runtime viewport`; `cargo nextest run -p jellyflow-runtime`
  Review: review-workstream before accepting completion.
  Evidence: `drag` and `selection` are split into focused scenario/support submodules while
  `viewport` and `auto_pan` remain compact direct test modules; targeted, package, and clippy
  gates pass.
  Context: docs/workstreams/jellyflow-runtime-test-surface-split-v1/CONTEXT.jsonl
  Handoff: DONE 2026-06-01. Test-only split completed without node drag, selection, viewport,
  auto-pan, store dispatch, callback, production API, or renderer-boundary changes.
  State: TASKS.jsonl entry JRTSS-030 records owner, scope, validation, evidence, and handoff status.

## M3 - Documentation And Closeout

- [x] JRTSS-040 [owner=codex] [deps=JRTSS-030] [scope=docs/workstreams/jellyflow-runtime-test-surface-split-v1]
  Goal: Record final evidence, check for stale task state, and close the lane or split follow-ons.
  Validation: `jq empty docs/workstreams/jellyflow-runtime-test-surface-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-runtime-test-surface-split-v1/TASKS.jsonl docs/workstreams/jellyflow-runtime-test-surface-split-v1/CAMPAIGNS.jsonl`; `git diff --check`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: Closeout audit records the test-only split, validation, residual risks, and any
  follow-on lanes.
  Context: docs/workstreams/jellyflow-runtime-test-surface-split-v1/CONTEXT.jsonl
  Handoff: DONE 2026-06-01. Closeout audit records the completed test-only split, final evidence,
  residual risks, and deferred follow-ons.
  State: TASKS.jsonl entry JRTSS-040 is verified or accepted before closeout.
