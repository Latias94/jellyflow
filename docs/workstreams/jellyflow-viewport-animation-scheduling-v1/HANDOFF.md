# Jellyflow Viewport Animation Scheduling v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

The workstream is open as a follow-on to the closed viewport interaction, viewport gesture policy,
and auto-pan lanes.

JVAS-010 is complete: the lane scope, non-goals, source coverage, task ledger, campaign record,
milestones, gate set, context manifest, and machine-readable workstream metadata are recorded.

JVAS-020 is complete: `runtime::viewport` now exposes renderer-neutral animation request/options,
easing, plan, and frame primitives. Focused tests cover cubic-in-out easing, linear easing,
zero-duration immediate completion, invalid time input, and broader viewport regression coverage.

## Active Task

- Task ID: JVAS-030
- Owner: codex
- Files: `crates/jellyflow-runtime/src/runtime/viewport`, `crates/jellyflow-runtime/src/runtime/tests/viewport`
- Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime double_click_zoom`
- Status: NEEDS_CONTEXT
- Review: review-workstream before accepting completion
- Evidence: focused viewport tests for accepted and rejected double-click zoom

## Decisions Since Opening

- Keep runtime animation scheduling pure: no timers, event loops, renderer dependencies, or async
  cancellation queue in `jellyflow-runtime`.
- Reuse existing `ViewportTransform` and anchored zoom math instead of creating a parallel viewport
  representation.
- Treat exact d3 `interpolateZoom` parity as optional unless focused tests prove it is needed for
  v1.
- Keep raw double-click detection in adapters; runtime should accept normalized double-click zoom
  input.
- Model animation duration in seconds, matching existing frame-time runtime APIs such as auto-pan.
- Add named runtime easing modes rather than accepting arbitrary adapter-provided functions.

## Blockers

- None known.

## Next Recommended Action

- Implement JVAS-030: normalized double-click zoom should reuse existing anchored zoom math and
  produce a viewport animation plan.
