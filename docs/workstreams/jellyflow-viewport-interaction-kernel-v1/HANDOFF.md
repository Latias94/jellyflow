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

JVI-030 is complete: `NodeGraphStore` applies normalized viewport pan/zoom intent through normal
view-state publication, viewport move gesture events are first-class runtime events, and the
XyFlow-style callback surface now dispatches `on_move_start`, `on_move`, and `on_move_end`.

JVI-040 is complete: the conformance fixture vocabulary can replay viewport pan/zoom intent,
record viewport store/view callbacks, and record viewport move gesture callbacks. The old adapter
viewport/selection ordering trace now runs through the fixture runner.

## Next Task

JVI-050: document viewport conformance, record fresh evidence, and close the lane or split
follow-ons.

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
- 2026-06-01: `cargo nextest run -p jellyflow-runtime viewport` passed, 14 tests run after JVI-030.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime explicit_modules_expose_their_owned_surfaces`
  passed, 1 test run after JVI-030.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime adapter_conformance` passed, 8 tests run
  after JVI-030.
- 2026-06-01: `cargo check -p jellyflow-runtime` passed after JVI-030.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime conformance` passed, 12 tests run after
  JVI-040.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime adapter_conformance` passed, 8 tests run
  after JVI-040.
- 2026-06-01: `cargo check -p jellyflow-runtime` passed after JVI-040.

## Follow-On Candidates

- Viewport animation/smoothing policy.
- Auto-pan integration with drag/select gestures.
- Adapter crate runner helpers for future wgpu, egui, Fret, or other integrations.
- Renderer smoke tests outside `jellyflow-runtime`.
