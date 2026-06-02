# Jellyflow Viewport Pan Inertia Scheduling v1 - Handoff

Status: Closed
Last updated: 2026-06-02

## Current State

The workstream is closed. It was opened as a follow-on to the closed viewport gesture policy,
viewport animation scheduling, and adapter template lanes.

JPIS-010 is complete: the lane scope, non-goals, source coverage, task ledger, campaign record,
milestones, gate set, context manifest, and machine-readable workstream metadata are recorded.

JPIS-020 is complete: `runtime::viewport` now exposes a renderer-neutral pan inertia request,
plan, frame, and planner. The planner samples adapter-provided logical screen px/s release velocity
with exponential decay, clamps initial speed, rejects invalid tuning/input, and converts screen
displacement to canvas pan by current zoom.

JPIS-030 is complete: conformance fixtures can apply and assert pan inertia frames, rejected
inertia plans can be fixture-asserted, adapter conformance proves view-state/callback trace replay,
and the headless adapter template includes a built-in pan inertia smoke scenario.

JPIS-040 is complete: README/runtime README document runtime-owned pan inertia planning versus
adapter-owned velocity estimation, frame clocks, cancellation, sampled-frame commits, and renderer
smoke; package and clippy gates passed; this workstream is closed with renderer-specific follow-ons
deferred.

## Next Task

None in this workstream. Follow-ons are split below.

## Decisions Since Opening

- Keep velocity estimation in adapters; runtime accepts normalized logical screen px/s velocity.
- Reuse existing `NodeGraphPanInertiaTuning` rather than adding new persisted config.
- Reuse `pan_viewport` semantics: screen deltas become canvas pan deltas by current zoom.
- Keep timers, frame loops, cancellation policy, renderer smoke, screenshots, and pixel assertions
  outside `jellyflow-runtime`.

## Blockers

- None known.

## Next Recommended Action

- Exact platform physics parity only if adapter integration proves the v1 exponential damping
  contract is not close enough.
- Renderer smoke, screenshot, pixel, or frame-loop helper tests for future wgpu, egui, Fret, or
  other adapters outside `jellyflow-runtime`.

## Closeout Evidence

- 2026-06-02: `cargo fmt --check` passed.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime` passed, 277 tests run.
- 2026-06-02: `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings` passed.
- 2026-06-02: `jq empty docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CONTEXT.jsonl`
  passed.
- 2026-06-02: `git diff --check` passed.
