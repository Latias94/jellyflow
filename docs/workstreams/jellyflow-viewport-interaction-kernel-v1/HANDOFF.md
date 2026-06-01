# Jellyflow Viewport Interaction Kernel v1 - Handoff

Status: Active
Last updated: 2026-06-01

## Current State

The workstream is open as a follow-on to the closed conformance fixture lane and the node drag
kernel lane.

JVI-010 is complete: the lane scope, non-goals, source coverage, task ledger, machine-readable task
state, draft campaign, milestones, gate set, context manifest, and workstream metadata are recorded.

JVI-020 is complete: `runtime::viewport` now exposes renderer-neutral pan/zoom request types and
deterministic transform helpers. Focused viewport tests cover drag-pan screen delta conversion,
zoom-around-pointer clamping/anchor stability, and invalid transform/input rejection. Public
surface coverage imports and exercises the new module.

## Next Task

JVI-030: wire viewport intent helpers through `NodeGraphStore` view-state publication and viewport
gesture/callback lifecycle events.

## Decisions Since Opening

- Runtime accepts normalized viewport intent; adapters own platform event capture.
- Keep v1 deterministic and immediate; animation and smoothing are follow-ons.
- Use the conformance fixture runner for viewport traces after the kernel and store callbacks exist.
- Renderer smoke tests stay outside `jellyflow-runtime`.

## Blockers

- None known.

## Latest Validation

- 2026-06-01: `cargo nextest run -p jellyflow-runtime viewport` passed, 12 tests run.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime explicit_modules_expose_their_owned_surfaces`
  passed, 1 test run.
- 2026-06-01: `cargo check -p jellyflow-runtime` passed.
- 2026-06-01: `cargo fmt --check` passed.

## Follow-On Candidates

- Viewport animation/smoothing policy.
- Auto-pan integration with drag/select gestures.
- Adapter crate runner helpers for future wgpu, egui, Fret, or other integrations.
- Renderer smoke tests outside `jellyflow-runtime`.
