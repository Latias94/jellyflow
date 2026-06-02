# Jellyflow Viewport Animation Scheduling v1

Status: Active
Last updated: 2026-06-02

## Why This Lane Exists

Jellyflow already has renderer-neutral viewport pan/zoom math, viewport gesture policy,
store-level view-state publication, viewport callbacks, auto-pan frames, and conformance fixtures.
The remaining XyFlow-feel gap is smooth viewport movement: animated viewport changes, double-click
zoom, and future pan inertia still have to be invented by each adapter.

XyFlow expresses this through panzoom options such as `duration`, `ease`, `interpolate`, and
`zoomOnDoubleClick`. Jellyflow should translate that idea into deterministic headless scheduling
contracts without depending on d3, DOM, browser timers, `wgpu`, egui, Fret, or a window event loop.

## Relevant Authority

- ADRs:
  - `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
  - `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- Project context:
  - `CONTEXT.md`
- Existing workstreams:
  - `docs/workstreams/jellyflow-viewport-interaction-kernel-v1/`
  - `docs/workstreams/jellyflow-viewport-gesture-policy-v1/`
  - `docs/workstreams/jellyflow-auto-pan-integration-v1/`
  - `docs/workstreams/jellyflow-adapter-conformance-runner-v1/`
- Reference implementation:
  - `repo-ref/xyflow/packages/system/src/xypanzoom/XYPanZoom.ts`
  - `repo-ref/xyflow/packages/system/src/xypanzoom/utils.ts`
  - `repo-ref/xyflow/packages/system/src/types/panzoom.ts`

## Problem

Immediate pan/zoom helpers prove the math for one viewport update, but they do not describe how a
smooth transition should advance across frames. Without a headless plan, adapters must each decide:

- how `duration=0` differs from animated transitions;
- how easing is sampled;
- how intermediate transforms are represented;
- how double-click zoom maps to an anchored zoom target;
- when the runtime emits start/update/end traces during an animated viewport change.

That duplication is a direct risk to XyFlow-like feel across Fret, egui, wgpu, and custom adapter
integrations.

## Target State

This lane closes when Jellyflow has a renderer-neutral viewport animation scheduling contract:

- pure animation request/plan/frame types under `runtime::viewport`;
- deterministic frame sampling from a current transform to a target transform;
- immediate behavior when duration is zero or invalid for animation;
- default cubic easing compatible with XyFlow's default transition feel;
- a double-click zoom planner that respects runtime zoom policy and reuses existing anchored zoom
  math;
- conformance coverage that lets adapter crates prove smooth viewport traces before renderer smoke;
- documentation that keeps timers, frame loops, cancellation policy, and rendered pixels outside
  the headless crates unless a future ADR says otherwise.

## In Scope

- Headless viewport animation plan primitives.
- Built-in interpolation modes needed by XyFlow-style options: linear first, with smooth behavior
  explicitly tested if implemented in this lane.
- Pure easing support through named runtime easing modes; adapters may still supply their own frame
  clock.
- Double-click zoom planning from normalized adapter input and current interaction state.
- Store or conformance integration only after the pure scheduling contract is stable.
- Public surface smoke for exported types.

## Out Of Scope

- Browser, DOM, d3, winit, egui, Fret, `wgpu`, screenshot, or pixel-test dependencies.
- Owning an event loop, async timer, frame callback, or cancellation queue inside
  `jellyflow-runtime`.
- Raw platform double-click detection.
- Exact d3 `interpolateZoom` parity unless the math can be proven with focused tests and kept
  renderer-neutral.
- Auto-pan frame scheduling changes.
- Persisted schema changes.

## Starting Assumptions

| Assumption | Confidence | Evidence | Consequence if wrong |
| --- | --- | --- | --- |
| Adapters can provide elapsed time or frame timestamps. | High | Auto-pan already accepts elapsed frame time. | Move only frame sampling into runtime and keep all scheduling in adapters. |
| Runtime should own transform interpolation math, not timers. | High | ADR 0003 keeps renderer/platform concerns outside runtime. | Split an adapter frame-loop helper lane. |
| Double-click zoom is behavior, not rendering. | High | XyFlow gates it as panzoom policy and calls zoom transforms. | Keep raw click detection in adapters but resolve zoom intent in runtime. |
| Default cubic easing is enough for first XyFlow-feel proof. | Medium | `xypanzoom/utils.ts` uses a default cubic ease for transitions. | Add a follow-on for exact smooth interpolation parity. |

## Architecture Direction

Add a deeper `runtime::viewport` submodule for animation scheduling. The module should sit above
`ViewportTransform`, `ViewportPanRequest`, and `ViewportZoomRequest`, but below store publication:

```text
adapter clock/input -> viewport animation plan -> frame transforms -> NodeGraphStore view state
```

The first code slice should be pure. It should not mutate a store, spawn timers, or decide adapter
event-loop behavior. Store and conformance integration should consume the same plan/frame types once
the pure math is proven.

## Source Coverage

| Source | State | Evidence | Impact |
| --- | --- | --- | --- |
| User goal | COVERED | Request to keep improving XyFlow-like headless feel | Makes smooth viewport behavior the next lane. |
| `CONTEXT.md` | COVERED | Top-level project context | Confirms renderer-free testing and follow-on lane candidates. |
| ADR 0003 | COVERED | `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md` | Keeps renderer smoke tests outside runtime. |
| Viewport interaction lane | COVERED | `docs/workstreams/jellyflow-viewport-interaction-kernel-v1/` | Provides immediate viewport pan/zoom and callbacks. |
| Viewport gesture policy lane | COVERED | `docs/workstreams/jellyflow-viewport-gesture-policy-v1/` | Provides normalized gesture gates and double-click policy field. |
| Auto-pan lane | COVERED | `docs/workstreams/jellyflow-auto-pan-integration-v1/` | Provides frame-time precedent while keeping scheduling adapter-owned. |
| XyFlow panzoom reference | COVERED | `repo-ref/xyflow/packages/system/src/xypanzoom/` | Provides duration/ease/interpolate and double-click behavior references. |

## Closeout Condition

This lane can close when:

- pure animation scheduling and double-click zoom planning are implemented or explicitly split;
- conformance or runtime tests prove the shipped behavior;
- public surface tests cover exported runtime types;
- README/runtime README explain adapter/runtime responsibilities;
- evidence gates pass with fresh command output;
- remaining inertia, exact d3 smooth interpolation, or renderer smoke work is split or deferred.
