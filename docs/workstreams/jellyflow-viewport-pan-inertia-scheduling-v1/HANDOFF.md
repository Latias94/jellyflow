# Jellyflow Viewport Pan Inertia Scheduling v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

The workstream is open as a follow-on to the closed viewport gesture policy, viewport animation
scheduling, and adapter template lanes.

JPIS-010 is complete: the lane scope, non-goals, source coverage, task ledger, campaign record,
milestones, gate set, context manifest, and machine-readable workstream metadata are recorded.

JPIS-020 is complete: `runtime::viewport` now exposes a renderer-neutral pan inertia request,
plan, frame, and planner. The planner samples adapter-provided logical screen px/s release velocity
with exponential decay, clamps initial speed, rejects invalid tuning/input, and converts screen
displacement to canvas pan by current zoom.

JPIS-030 is complete: conformance fixtures can apply and assert pan inertia frames, rejected
inertia plans can be fixture-asserted, adapter conformance proves view-state/callback trace replay,
and the headless adapter template includes a built-in pan inertia smoke scenario.

## Active Task

- Task ID: JPIS-040
- Owner: codex
- Files: `README.md`, `crates/jellyflow-runtime/README.md`, `docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1`
- Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`; `jq empty docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CONTEXT.jsonl`; `git diff --check`
- Status: NEEDS_CONTEXT
- Review: review-workstream and verify-rust-workstream before closeout
- Evidence: README/runtime README boundary notes, closeout evidence, and machine-readable state

## Decisions Since Opening

- Keep velocity estimation in adapters; runtime accepts normalized logical screen px/s velocity.
- Reuse existing `NodeGraphPanInertiaTuning` rather than adding new persisted config.
- Reuse `pan_viewport` semantics: screen deltas become canvas pan deltas by current zoom.
- Keep timers, frame loops, cancellation policy, renderer smoke, screenshots, and pixel assertions
  outside `jellyflow-runtime`.

## Blockers

- None known.

## Next Recommended Action

- Implement JPIS-040: document pan inertia ownership boundaries, run package and clippy gates, then
  close or split remaining renderer-specific follow-ons.
