# Jellyflow Viewport Animation Scheduling v1 - TODO

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

- [x] JVAS-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-viewport-animation-scheduling-v1]
  Goal: Open the viewport animation scheduling workstream from viewport smoothing and double-click
  zoom follow-ons.
  Validation: DESIGN.md, TODO.md, MILESTONES.md, EVIDENCE_AND_GATES.md, CONTEXT.jsonl,
  WORKSTREAM.json, TASKS.jsonl, and CAMPAIGNS.jsonl exist and agree.
  Review: planner self-review for artifact agreement.
  Evidence: docs/workstreams/jellyflow-viewport-animation-scheduling-v1/DESIGN.md
  Context: docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CONTEXT.jsonl
  Handoff: DONE 2026-06-02. Lane opened with JVAS-020 as the first executable task.
  State: TASKS.jsonl entry JVAS-010 matches this task.

## M1 - Pure Animation Planner

- [x] JVAS-020 [owner=codex] [deps=JVAS-010] [scope=crates/jellyflow-runtime/src/runtime/viewport,crates/jellyflow-runtime/src/runtime/tests/viewport,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Add renderer-neutral viewport animation request/plan/frame primitives that interpolate
  between viewport transforms with deterministic easing and immediate zero-duration behavior.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime viewport_animation; cargo
  nextest run -p jellyflow-runtime --test public_surface
  Review: review-workstream before accepting completion.
  Evidence: runtime viewport animation tests and public surface smoke.
  Context: docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CONTEXT.jsonl plus XyFlow
  panzoom transition references.
  Handoff: DONE 2026-06-02. Added `ViewportAnimationRequest`, `ViewportAnimationOptions`,
  `ViewportAnimationEasing`, `ViewportAnimationPlan`, and `ViewportAnimationFrame`, with focused
  tests for cubic easing, linear easing, immediate zero-duration behavior, and invalid time input.
  State: TASKS.jsonl entry JVAS-020 records owner, scope, validation, evidence, and handoff status.

## M2 - Double-Click Zoom Plan

- [ ] JVAS-030 [owner=codex] [deps=JVAS-020] [scope=crates/jellyflow-runtime/src/runtime/viewport,crates/jellyflow-runtime/src/runtime/tests/viewport]
  Goal: Resolve normalized double-click zoom input into an anchored viewport animation plan that
  respects `zoom_on_double_click`, min/max zoom, invalid input, and existing zoom math.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime double_click_zoom
  Review: review-workstream before accepting completion.
  Evidence: focused viewport tests for accepted and rejected double-click zoom.
  Context: docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CONTEXT.jsonl.
  Handoff: Split raw double-click detection or adapter event-loop behavior if scope expands.
  State: TASKS.jsonl entry JVAS-030 records owner, scope, validation, evidence, and handoff status.

## M3 - Conformance Trace Integration

- [ ] JVAS-040 [owner=codex] [deps=JVAS-030] [scope=crates/jellyflow-runtime/src/runtime/conformance,crates/jellyflow-runtime/src/runtime/tests/adapter_conformance]
  Goal: Let conformance fixtures assert viewport animation plans and double-click zoom traces
  without renderer or timer dependencies.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime conformance; cargo nextest
  run -p jellyflow-runtime adapter_conformance
  Review: review-workstream before accepting completion.
  Evidence: fixture-runner traces for viewport animation planning.
  Context: docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CONTEXT.jsonl.
  Handoff: Keep frame-loop helpers and renderer smoke in adapter crates.
  State: TASKS.jsonl entry JVAS-040 records owner, scope, validation, evidence, and handoff status.

## M4 - Documentation And Closeout

- [ ] JVAS-050 [owner=codex] [deps=JVAS-040] [scope=README.md,crates/jellyflow-runtime/README.md,docs/workstreams/jellyflow-viewport-animation-scheduling-v1]
  Goal: Document viewport animation scheduling boundaries, record fresh evidence, and close or
  split follow-ons.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime; cargo clippy -p
  jellyflow-runtime --all-targets -- -D warnings; jq empty
  docs/workstreams/jellyflow-viewport-animation-scheduling-v1/WORKSTREAM.json
  docs/workstreams/jellyflow-viewport-animation-scheduling-v1/TASKS.jsonl
  docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CAMPAIGNS.jsonl
  docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CONTEXT.jsonl; git diff --check
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: EVIDENCE_AND_GATES.md, WORKSTREAM.json, and closeout audit.
  Context: docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CONTEXT.jsonl.
  Handoff: Summarize residual risks and follow-ons.
  State: TASKS.jsonl entry JVAS-050 is verified or accepted before closeout.
