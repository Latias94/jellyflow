# ADR 0003: Headless Adapter Testing and Renderer Boundary

Status: Accepted
Date: 2026-06-01

## Context

Jellyflow is a headless node graph engine intended to be embedded by renderer or UI adapters such
as Fret, egui, wgpu-backed shells, or other Rust self-drawn frameworks. Consumers still expect a
graph editor to feel like XyFlow: predictable connect/reconnect/delete behavior, coherent viewport
and selection events, stable gesture policy, deterministic node/edge change projection, and usable
geometry/hit-test contracts.

That feel is not primarily a renderer property. It is the combination of:

- graph operation semantics,
- runtime store commit/event ordering,
- interaction policy resolution,
- geometry and hit testing,
- XyFlow-style lossy projections for adapter-facing node/edge updates,
- adapter translation from pointer/keyboard input into headless runtime calls.

Adding a renderer dependency to the headless crates would make the primary package boundary less
portable and contradict the existing decision that `jellyflow-core` and `jellyflow-runtime` remain
free of Fret UI, renderer, platform, `wgpu`, and `winit` dependencies.

## Decision

Keep `jellyflow-core` and `jellyflow-runtime` renderer-free. Do not add `wgpu` to either crate.

Test Jellyflow behavior in layers:

1. **Core model contracts**: graph validation, transactions, diff/invert/normalize, fragments,
   imports, symbols, and type compatibility.
2. **Runtime contracts**: store dispatch, undo/redo, middleware, subscriptions, view state,
   lookups, policy, geometry, fit-view, and hit testing.
3. **XyFlow adapter contracts**: node/edge change projection, change-to-transaction conversion,
   callback ordering, and controlled-mode store integration under `runtime::xyflow`.
4. **Adapter conformance scenarios**: renderer-independent gesture fixtures that describe pointer
   and keyboard intent, expected graph transactions, expected view/selection state, expected
   callbacks, and expected geometry hit targets.
5. **Renderer smoke tests**: optional adapter crates or examples may run wgpu/egui/Fret render
   smoke tests, screenshots, or pixel checks. These tests belong outside `jellyflow-core` and
   `jellyflow-runtime`.
6. **External usage smoke**: examples and external temporary crates must prove that consumers can
   use canonical runtime APIs without Fret or renderer dependencies.

A future `jellyflow-wgpu`, `jellyflow-egui`, or `jellyflow-fret` adapter can depend on renderer and
platform crates. Such an adapter may own renderer-specific test harnesses, screenshots, and pixel
checks, but it must consume Jellyflow through the public headless interfaces.

## Consequences

- The headless crates stay portable for non-GPU environments, server-side validation, tests,
  editor state manipulation, and alternative renderers.
- XyFlow feel is tested through behavior contracts first, not by requiring a browser or GPU.
- Renderer-specific regressions are still testable, but the dependency cost is paid by adapter
  crates that actually need rendering.
- The next useful testing investment is an adapter conformance fixture format, not a wgpu
  dependency inside the runtime crate.

