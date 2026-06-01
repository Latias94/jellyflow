# Jellyflow Auto-Pan Integration v1 - Design

Status: Closed
Date: 2026-06-01

## Problem

Jellyflow now has renderer-neutral node dragging, selection boxes, viewport pan/zoom helpers, and
conformance fixtures. The remaining XyFlow-feel gap is auto-pan: while a drag/connect/select
workflow approaches a viewport edge, adapters need deterministic runtime guidance for how far to
move the viewport each frame.

If every adapter computes edge proximity, speed, and view-state publication locally, Fret, egui,
wgpu, and other Rust integrations will drift. Auto-pan belongs at the headless runtime boundary as
screen-space intent math and store publication, while raw pointer capture, timers, wheel/pinch
normalization, and rendered smoke tests remain adapter-owned.

## Target State

- `jellyflow-runtime` exposes a small renderer-neutral `runtime::auto_pan` module.
- Auto-pan requests use logical screen coordinates, viewport size, elapsed frame time, and resolved
  `NodeGraphAutoPanTuning`.
- The kernel produces deterministic screen-pixel pan deltas that feed the existing
  `ViewportPanRequest` path.
- Store helpers publish auto-pan through normal view-state events and XyFlow-compatible viewport
  callbacks.
- Conformance fixtures can replay an auto-pan frame and assert viewport/callback ordering.
- Documentation clearly states that adapters own frame scheduling and platform input capture.

## Scope

- Add the public auto-pan request/result types and pure computation helper.
- Reuse existing `NodeGraphAutoPanTuning` fields instead of adding persisted configuration unless a
  later task proves the existing policy is insufficient.
- Add store-level application through existing viewport publication.
- Add focused runtime tests and public-surface coverage.
- Add conformance coverage after the kernel shape is stable.

## Non-Goals

- No `wgpu`, `winit`, egui, Fret, DOM, or renderer dependencies in `jellyflow-runtime`.
- No platform timer/event-loop abstraction in this lane.
- No animated viewport smoothing policy; frame-by-frame auto-pan should remain deterministic.
- No raw wheel, pinch, or pointer-capture normalization.
- No breaking change to persisted graph schema.

## Assumptions

- Logical screen pixels are the adapter/runtime contract for pointer proximity and viewport size.
- `ViewportPanRequest` remains the single path for changing viewport pan from auto-pan.
- `NodeGraphAutoPanTuning::speed` is screen pixels per second and `margin` is a screen-pixel edge
  activation band.
- Existing `on_node_drag`, `on_connect`, and `on_node_focus` toggles are enough for the first
  integrated runtime policy. Selection workflows may call the generic kernel directly until a
  dedicated persisted selection toggle is justified.

## Architecture Direction

The kernel should be pure and small:

1. validate tuning, viewport size, pointer, and elapsed seconds;
2. calculate edge intensity independently per axis;
3. return a screen-space content movement delta for the current frame;
4. let `NodeGraphStore::apply_viewport_pan` publish the resulting viewport update.

This preserves ADR 0003: renderer smoke tests stay outside the headless runtime, while XyFlow feel
is proven with behavior contracts and conformance traces.

## Risks

- Coordinate sign ambiguity is the main correctness risk. Tests must assert that a pointer near the
  right/bottom edge reveals content to the right/bottom by moving rendered content left/up.
- Very small viewport sizes can make both edges active. The kernel should choose a deterministic
  nearest-edge interpretation and avoid exploding speed.
- Adding a selection-specific persisted toggle too early would widen the config contract without
  evidence. Keep the first pass generic.
