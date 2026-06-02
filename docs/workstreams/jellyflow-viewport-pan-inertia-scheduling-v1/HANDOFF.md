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

## Active Task

- Task ID: JPIS-030
- Owner: codex
- Files: `crates/jellyflow-runtime/src/runtime/conformance`, `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance`, `templates/headless-adapter`
- Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime conformance`; `cargo nextest run -p jellyflow-runtime adapter_conformance`; `cargo test --manifest-path templates/headless-adapter/Cargo.toml`; `cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check`
- Status: NEEDS_CONTEXT
- Review: review-workstream before accepting completion
- Evidence: conformance/template inertia traces through normal view-state publication

## Decisions Since Opening

- Keep velocity estimation in adapters; runtime accepts normalized logical screen px/s velocity.
- Reuse existing `NodeGraphPanInertiaTuning` rather than adding new persisted config.
- Reuse `pan_viewport` semantics: screen deltas become canvas pan deltas by current zoom.
- Keep timers, frame loops, cancellation policy, renderer smoke, screenshots, and pixel assertions
  outside `jellyflow-runtime`.

## Blockers

- None known.

## Next Recommended Action

- Implement JPIS-030: extend conformance fixtures and the headless adapter template so sampled
  inertia frames replay through the normal view-state publication path without moving frame loops
  into runtime.
