# Jellyflow Viewport Pan Inertia Scheduling v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

The workstream is open as a follow-on to the closed viewport gesture policy, viewport animation
scheduling, and adapter template lanes.

JPIS-010 is complete: the lane scope, non-goals, source coverage, task ledger, campaign record,
milestones, gate set, context manifest, and machine-readable workstream metadata are recorded.

## Active Task

- Task ID: JPIS-020
- Owner: codex
- Files: `crates/jellyflow-runtime/src/runtime/viewport`, `crates/jellyflow-runtime/src/runtime/tests/viewport`, `crates/jellyflow-runtime/tests/public_surface.rs`
- Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime pan_inertia`; `cargo nextest run -p jellyflow-runtime --test public_surface`
- Status: NEEDS_CONTEXT
- Review: review-workstream before accepting completion
- Evidence: focused runtime tests for pan inertia planning

## Decisions Since Opening

- Keep velocity estimation in adapters; runtime accepts normalized logical screen px/s velocity.
- Reuse existing `NodeGraphPanInertiaTuning` rather than adding new persisted config.
- Reuse `pan_viewport` semantics: screen deltas become canvas pan deltas by current zoom.
- Keep timers, frame loops, cancellation policy, renderer smoke, screenshots, and pixel assertions
  outside `jellyflow-runtime`.

## Blockers

- None known.

## Next Recommended Action

- Implement JPIS-020: add pure pan inertia planner/frame types under `runtime::viewport` with
  focused tests for enabled/disabled tuning, velocity clamp, exponential damping, stop threshold,
  invalid input, and public surface exposure.
