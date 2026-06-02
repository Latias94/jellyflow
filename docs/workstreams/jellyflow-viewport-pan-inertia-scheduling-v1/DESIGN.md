# Jellyflow Viewport Pan Inertia Scheduling v1

Status: Closed
Last updated: 2026-06-02

## Why This Lane Exists

Jellyflow already has renderer-neutral viewport pan/zoom math, viewport gesture policy, animation
plans, double-click zoom planning, and conformance actions for sampled viewport animation frames.
The remaining smooth-viewport follow-on is inertial panning after a pan drag ends.

The data model already exposes `NodeGraphPanInertiaTuning`, and gesture events already include
`ViewportMoveKind::PanInertia`. Without a runtime planner, adapters still have to each invent how
screen velocity decays into viewport pan frames.

## Relevant Authority

- `CONTEXT.md`
- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `docs/workstreams/jellyflow-viewport-gesture-policy-v1/`
- `docs/workstreams/jellyflow-viewport-animation-scheduling-v1/`
- `docs/workstreams/jellyflow-adapter-template-v1/`

## Problem

Adapters can detect pointer release and estimate pan velocity, but the runtime currently has no
deterministic contract for converting that velocity into frame-by-frame viewport changes. That
duplicates:

- velocity clamping;
- disabled/min-speed handling;
- exponential decay math;
- screen-pixel velocity to canvas-space pan conversion at the current zoom;
- conformance fixture vocabulary for pan inertia traces.

## Target State

This lane closes when Jellyflow has a renderer-neutral pan inertia scheduling contract:

- pure runtime request/plan/frame types under `runtime::viewport`;
- deterministic frame sampling from current transform, initial screen velocity, and
  `NodeGraphPanInertiaTuning`;
- no runtime-owned timers, event loops, cancellation queues, pointer capture, or renderer code;
- conformance coverage that can replay inertia frames through the normal view-state publication
  path;
- adapter template coverage that proves external adapters can exercise pan inertia before renderer
  smoke tests;
- README/runtime README guidance for the adapter/runtime split.

## In Scope

- Headless pan inertia planner and frame sampling.
- Existing `NodeGraphPanInertiaTuning` defaults and validation.
- Conformance actions for sampled inertia frames.
- Public surface smoke for exported planner and conformance vocabulary.
- Adapter template smoke coverage.

## Out Of Scope

- Raw pointer velocity estimation.
- Runtime timers, frame callbacks, async tasks, or cancellation queues.
- Renderer, `wgpu`, egui, Fret, screenshot, or pixel-test dependencies.
- Exact browser, d3, or platform physics parity unless an adapter integration proves it is needed.
- Changing immediate viewport pan/zoom behavior.

## Starting Assumptions

| Assumption | Confidence | Evidence | Consequence if wrong |
| --- | --- | --- | --- |
| Adapters can estimate release velocity in logical screen px/s. | High | Pointer adapters already own raw input and frame clocks. | Keep velocity estimation outside runtime and only accept normalized velocity. |
| Runtime should convert screen velocity to canvas pan by current zoom. | High | `pan_viewport` already converts screen delta by zoom. | Reuse the same pan semantics for inertia frames. |
| Existing `NodeGraphPanInertiaTuning` is the right tuning source. | High | It already lives in persisted interaction config and resolved pan interaction. | Avoid new config fields unless tests reveal a missing invariant. |
| Exponential damping is sufficient for v1. | Medium | Tuning already names `decay_per_s`. | Split exact platform feel as a follow-on if adapter evidence requires it. |

## Architecture Direction

Add pan inertia scheduling under `runtime::viewport`, beside animation and gesture policy:

```text
adapter release velocity -> pan inertia plan -> sampled viewport frames -> NodeGraphStore view state
```

The planner should be pure and deterministic. Adapters still decide when to sample, when to stop,
how to estimate velocity, and how to cancel inertia when a new gesture starts.

## Source Coverage

| Source | State | Evidence | Impact |
| --- | --- | --- | --- |
| User goal | COVERED | Continue XyFlow-feel headless architecture work | Makes pan inertia a natural next smooth viewport lane. |
| `CONTEXT.md` | COVERED | Lists pan inertia as a headless scheduling contract | Confirms this lane belongs in runtime planning, not renderer code. |
| ADR 0003 | COVERED | Keeps renderer smoke outside runtime | Prevents wgpu/egui/Fret dependencies here. |
| `NodeGraphPanInertiaTuning` | COVERED | Existing config/tuning type | Reuse existing persisted policy. |
| `ViewportMoveKind::PanInertia` | COVERED | Existing gesture kind | Conformance can use existing callback vocabulary. |
| Animation scheduling lane | COVERED | Recent pure plan/frame precedent | Reuse pure plan/frame shape and adapter-owned clock split. |

## Closeout Condition

This lane can close when:

- the pure inertia planner is implemented or explicitly split;
- runtime/conformance/template tests prove accepted and rejected inertia behavior;
- public surface tests cover exported types/actions;
- README/runtime README explain adapter-owned velocity estimation and frame scheduling;
- fresh package, clippy, JSON, and diff gates pass;
- remaining platform-specific physics parity or renderer smoke work is split or deferred.
