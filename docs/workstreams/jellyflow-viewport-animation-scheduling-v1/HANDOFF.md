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

JVAS-030 is complete: normalized double-click zoom input now resolves to anchored viewport
animation plans, respects `zoom_on_double_click`, uses existing min/max zoom clamp behavior, and
rejects invalid normalized input without adding raw platform event handling.

JVAS-040 is complete: conformance fixtures now expose renderer-free assertions for sampled viewport
animation frames and double-click zoom plan or rejection outcomes. The adapter conformance fixture
runner can use the same assertions while leaving render traces empty for pure planning checks.

## Active Task

- Task ID: JVAS-050
- Owner: codex
- Files: `README.md`, `crates/jellyflow-runtime/README.md`, `docs/workstreams/jellyflow-viewport-animation-scheduling-v1`
- Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`; `jq empty docs/workstreams/jellyflow-viewport-animation-scheduling-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-animation-scheduling-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CONTEXT.jsonl`; `git diff --check`
- Status: NEEDS_CONTEXT
- Review: review-workstream and verify-rust-workstream before closeout
- Evidence: README/runtime README guidance, closeout audit, and fresh package gates

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
- Model double-click zoom as normalized runtime input with a zoom factor; adapters own raw
  double-click detection and any platform-specific modifier semantics.

## Blockers

- None known.

## Next Recommended Action

- Implement JVAS-050: document viewport animation scheduling boundaries, run package closeout
  gates, and close or split follow-ons.
