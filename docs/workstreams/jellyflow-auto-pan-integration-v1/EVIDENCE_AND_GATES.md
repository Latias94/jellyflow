# Jellyflow Auto-Pan Integration v1 - Evidence And Gates

Status: Closed
Last updated: 2026-06-01

## Smallest Current Repro

An adapter can drag a node near the viewport edge, but the runtime has no canonical headless API for
turning pointer-edge proximity and elapsed frame time into deterministic viewport panning.

## Required Gates

- `cargo nextest run -p jellyflow-runtime auto_pan`
- `cargo nextest run -p jellyflow-runtime conformance`
- `cargo nextest run -p jellyflow-runtime adapter_conformance`
- `cargo check -p jellyflow-runtime`
- `cargo fmt --check`
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
- `jq empty docs/workstreams/jellyflow-auto-pan-integration-v1/WORKSTREAM.json docs/workstreams/jellyflow-auto-pan-integration-v1/TASKS.jsonl docs/workstreams/jellyflow-auto-pan-integration-v1/CAMPAIGNS.jsonl`
- `git diff --check`

## Evidence Log

- 2026-06-01: JAI-010 opened the auto-pan integration lane as a follow-on to node drag and viewport
  interaction work.
  - Scope keeps renderer/platform frame scheduling outside `jellyflow-runtime`.
  - Target path is a pure runtime kernel feeding existing `ViewportPanRequest` publication.
- 2026-06-01: JAI-020 added the renderer-neutral auto-pan kernel and store helper.
  - Added `runtime::auto_pan::{AutoPanActivation, AutoPanRequest, AutoPanPlan, AutoPanOutcome}`.
  - Added `compute_auto_pan` and `NodeGraphStore::apply_auto_pan`.
  - Focused tests cover right/bottom sign convention, workflow activation policy, invalid/no-op
    frames, and store view-state publication.
  - Public-surface smoke coverage constructs the auto-pan API.
  - `cargo nextest run -p jellyflow-runtime auto_pan`: passed, 4 tests.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 3 tests.
  - `cargo check -p jellyflow-runtime`: passed.
- 2026-06-01: JAI-030 added auto-pan conformance fixture coverage.
  - Added `ConformanceAction::apply_auto_pan` and runner execution through
    `NodeGraphStore::apply_auto_pan`.
  - Added a callback-aware conformance scenario asserting viewport view change, `onChange`, and
    `onViewportChange` ordering for one auto-pan frame.
  - Added adapter-conformance fixture coverage for auto-pan frame replay.
  - `cargo nextest run -p jellyflow-runtime conformance`: passed, 14 tests.
  - `cargo nextest run -p jellyflow-runtime adapter_conformance`: passed, 9 tests.
  - `cargo check -p jellyflow-runtime`: passed.
- 2026-06-01: JAI-040 closed the auto-pan integration workstream.
  - `cargo fmt --check`: passed.
  - `cargo nextest run -p jellyflow-runtime`: 165 passed, 0 skipped.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `jq empty docs/workstreams/jellyflow-auto-pan-integration-v1/WORKSTREAM.json docs/workstreams/jellyflow-auto-pan-integration-v1/TASKS.jsonl docs/workstreams/jellyflow-auto-pan-integration-v1/CAMPAIGNS.jsonl`: passed.
  - `git diff --check`: passed.
  - Documentation: `README.md`, `crates/jellyflow-runtime/README.md`, and
    `CLOSEOUT_AUDIT_2026-06-01.md`.

## Notes

This workstream is closed. Follow-ons are split below in `HANDOFF.md` and the closeout audit.
