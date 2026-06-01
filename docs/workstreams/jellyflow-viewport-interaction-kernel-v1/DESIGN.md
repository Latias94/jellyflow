# Jellyflow Viewport Interaction Kernel v1 - Design

Status: Closed
Date: 2026-06-01

## Problem

Jellyflow has headless contracts for selection, connect traces, node drag, geometry, and reusable
conformance fixtures. Viewport pan/zoom is still mostly represented as direct `set_viewport` state
updates. That is enough for storage, but not enough to prove XyFlow-like viewport feel across future
Fret, egui, wgpu, or custom adapters.

Without a renderer-free viewport interaction kernel, adapters must each decide how wheel, pinch,
drag-pan, and zoom-around-pointer intent maps to pan/zoom state and callback ordering.

## Target State

Create a headless viewport interaction kernel that can be tested before renderer smoke tests:

- deterministic pan/zoom request types and math helpers;
- store-level helpers that apply viewport intent through normal view-state publication;
- gesture lifecycle events and XyFlow-compatible callbacks for viewport movement;
- conformance fixtures that replay viewport pan/zoom scenarios against `NodeGraphStore`;
- README material that tells adapter authors where headless viewport conformance ends and renderer
  input/pixel smoke begins.

## Scope

- Add renderer-neutral viewport pan/zoom helpers under the runtime crate.
- Reuse existing `NodeGraphViewState` sanitization and `NodeGraphStore::set_viewport` publication.
- Add or expose viewport gesture payloads in the same spirit as node drag gesture payloads.
- Add conformance scenarios for viewport movement.
- Keep all behavior deterministic and canvas-space oriented.

## Non-Goals

- Do not add `wgpu`, `winit`, egui, Fret UI, screenshot, pixel-test, browser, or DOM dependencies.
- Do not own platform pointer capture, wheel delta normalization, touchpad gesture detection, or
  OS/windowing event loops in `jellyflow-runtime`.
- Do not implement visual smooth scrolling or animation timelines in this lane.
- Do not solve adapter-specific cursor, CSS, or hit-target behavior.

## Architecture Direction

The viewport kernel should sit next to `runtime::drag`, above view-state storage and below adapter
input capture:

```text
adapter input intent -> viewport kernel -> NodeGraphStore view state -> gesture/callback trace
```

Adapters normalize platform events into viewport intent. Jellyflow owns deterministic math, state
publication, callback ordering, and conformance traces.

## Source Coverage

| Source | State | Evidence | Impact |
| --- | --- | --- | --- |
| User goal | COVERED | Request for XyFlow-like headless test harnesses before renderer work | Viewport feel becomes the next conformance target. |
| ADR 0003 | COVERED | `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md` | Keeps renderer and platform dependencies out of runtime. |
| Conformance fixtures lane | COVERED | `docs/workstreams/jellyflow-conformance-fixtures-v1/CLOSEOUT_AUDIT_2026-06-01.md` | Supplies fixture vocabulary and runner for viewport traces. |
| Node drag kernel lane | COVERED | `docs/workstreams/jellyflow-node-drag-kernel-v1/CLOSEOUT_AUDIT_2026-06-01.md` | Supplies pattern for renderer-neutral interaction kernels. |
| Geometry spatial lane | COVERED | `docs/workstreams/jellyflow-geometry-spatial-v1/CLOSEOUT_AUDIT_2026-06-01.md` | Supplies geometry and viewport math context. |

## Risk Notes

- Wheel/pinch delta semantics vary by platform. Runtime should accept normalized intent, not raw
  platform events.
- A too-broad viewport API can become animation policy. Keep v1 to deterministic immediate state
  transforms.
- Callback ordering should mirror XyFlow enough for adapters without coupling to React or DOM.
