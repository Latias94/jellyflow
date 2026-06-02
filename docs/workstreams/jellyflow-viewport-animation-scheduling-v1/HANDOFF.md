# Jellyflow Viewport Animation Scheduling v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

The workstream is open as a follow-on to the closed viewport interaction, viewport gesture policy,
and auto-pan lanes.

JVAS-010 is complete: the lane scope, non-goals, source coverage, task ledger, campaign record,
milestones, gate set, context manifest, and machine-readable workstream metadata are recorded.

## Active Task

- Task ID: JVAS-020
- Owner: codex
- Files: `crates/jellyflow-runtime/src/runtime/viewport`, `crates/jellyflow-runtime/src/runtime/tests/viewport`, `crates/jellyflow-runtime/tests/public_surface.rs`
- Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime viewport_animation`; `cargo nextest run -p jellyflow-runtime --test public_surface`
- Status: NEEDS_CONTEXT
- Review: review-workstream before accepting completion
- Evidence: runtime viewport animation tests and public surface smoke

## Decisions Since Opening

- Keep runtime animation scheduling pure: no timers, event loops, renderer dependencies, or async
  cancellation queue in `jellyflow-runtime`.
- Reuse existing `ViewportTransform` and anchored zoom math instead of creating a parallel viewport
  representation.
- Treat exact d3 `interpolateZoom` parity as optional unless focused tests prove it is needed for
  v1.
- Keep raw double-click detection in adapters; runtime should accept normalized double-click zoom
  input.

## Blockers

- None known.

## Next Recommended Action

- Implement JVAS-020 as the smallest pure viewport animation planner slice.
