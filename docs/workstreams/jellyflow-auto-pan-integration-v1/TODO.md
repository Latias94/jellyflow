# Jellyflow Auto-Pan Integration v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JAI-010 [owner=codex] [scope=docs/workstreams/jellyflow-auto-pan-integration-v1]
  Goal: Open the auto-pan integration workstream from viewport and node-drag follow-ons.
  Validation: `jq empty docs/workstreams/jellyflow-auto-pan-integration-v1/WORKSTREAM.json docs/workstreams/jellyflow-auto-pan-integration-v1/TASKS.jsonl docs/workstreams/jellyflow-auto-pan-integration-v1/CAMPAIGNS.jsonl`
  Review: planner self-review for artifact agreement.
  Evidence: Workstream docs, task ledger, context manifest, and gates are created.

## M1 - Runtime Kernel

- [x] JAI-020 [owner=codex] [deps=JAI-010] [scope=crates/jellyflow-runtime/src/runtime/auto_pan.rs,crates/jellyflow-runtime/src/runtime/mod.rs,crates/jellyflow-runtime/src/runtime/tests/auto_pan.rs,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Add a renderer-neutral auto-pan kernel and store helper that converts pointer-edge
  proximity into deterministic viewport pan frames.
  Validation: `cargo nextest run -p jellyflow-runtime auto_pan`; `cargo check -p jellyflow-runtime`
  Review: review-workstream before accepting completion.
  Evidence: Focused auto-pan tests and public-surface smoke coverage.
  Handoff: DONE 2026-06-01. Added `runtime::auto_pan`, `NodeGraphStore::apply_auto_pan`, focused
  tests for direction/policy/invalid/store publication, and public-surface coverage.

## M2 - Conformance Fixture

- [x] JAI-030 [owner=codex] [deps=JAI-020] [scope=crates/jellyflow-runtime/src/runtime/conformance,crates/jellyflow-runtime/src/runtime/tests/conformance.rs,crates/jellyflow-runtime/src/runtime/tests/adapter_conformance.rs]
  Goal: Add conformance actions/traces for one auto-pan frame through store view-state publication
  and XyFlow-style viewport callbacks.
  Validation: `cargo nextest run -p jellyflow-runtime conformance`; `cargo nextest run -p jellyflow-runtime adapter_conformance`; `cargo check -p jellyflow-runtime`
  Review: review-workstream before accepting completion.
  Evidence: Fixture-runner trace asserts auto-pan viewport movement and callback ordering.
  Handoff: DONE 2026-06-01. Added `ConformanceAction::apply_auto_pan`, runner execution, a
  callback-aware conformance trace, and adapter-conformance fixture coverage.

## M3 - Workflow Guidance And Closeout

- [x] JAI-040 [owner=codex] [deps=JAI-030] [scope=README.md,crates/jellyflow-runtime/README.md,docs/workstreams/jellyflow-auto-pan-integration-v1]
  Goal: Document adapter responsibilities for scheduling auto-pan during node drag, connect, and
  selection workflows; record fresh evidence; close the lane or split follow-ons.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`; `jq empty docs/workstreams/jellyflow-auto-pan-integration-v1/WORKSTREAM.json`; `git diff --check`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: Runtime README explains the auto-pan boundary and follow-ons are explicit.
  Handoff: DONE 2026-06-01. Updated README/runtime README, recorded closeout audit and final
  verification evidence, closed the workstream, and split follow-ons for selection-specific
  persisted policy, viewport smoothing, and adapter frame-loop helpers.
