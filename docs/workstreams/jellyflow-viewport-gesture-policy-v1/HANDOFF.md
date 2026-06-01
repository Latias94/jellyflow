# Jellyflow Viewport Gesture Policy v1 - Handoff

Status: Closed
Last updated: 2026-06-01

## Current State

This lane is closed.

Runtime now has renderer-neutral viewport gesture policy types and pure resolvers for scroll/pinch
and drag-pan decisions in `crates/jellyflow-runtime/src/runtime/viewport.rs`. Conformance fixtures
can execute accepted policy actions or assert expected rejections, and public surface smoke covers
the exported vocabulary.

## Next Task

Open a follow-on lane for pan inertia scheduling, double-click zoom animation, or adapter renderer
smoke tests if those become the next priority.

## Important Constraints

- Do not add renderer, platform, DOM, d3, winit, egui, Fret, wgpu, or windowing dependencies to
  `jellyflow-core` or `jellyflow-runtime`.
- Keep existing viewport pan/zoom math behavior stable unless a test makes the intended change
  explicit.
- Use normalized adapter input values only.
- Preserve XyFlow naming only where it is compatibility-facing; canonical runtime names should be
  Jellyflow names.

## Risks

- Policy input types can become too DOM-shaped. Keep them semantic and renderer-neutral.
- Do not add renderer/platform dependencies to `jellyflow-core` or `jellyflow-runtime`.
- Keep raw event normalization in adapter crates; runtime policy should accept normalized values.
