# Jellyflow Viewport Gesture Policy v1

Status: Active
Last updated: 2026-06-01

## Why This Lane Exists

Jellyflow already has renderer-neutral viewport math, auto-pan math, gesture events, and
conformance traces. What is still missing is the headless decision point that turns normalized
adapter input into viewport pan/zoom intent. XyFlow centralizes this in `xypanzoom/filter.ts` and
`xypanzoom/eventhandler.ts`; Jellyflow currently leaves equivalent rules for each adapter to copy.

## Relevant Authority

- ADRs:
  - `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
  - `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- Existing docs:
  - `docs/workstreams/jellyflow-viewport-interaction-kernel-v1/`
  - `docs/workstreams/jellyflow-auto-pan-integration-v1/`
  - `docs/workstreams/jellyflow-interaction-harness-v1/`
  - `docs/workstreams/jellyflow-conformance-fixture-discovery-v1/`
- Reference implementation:
  - `repo-ref/xyflow/packages/system/src/xypanzoom/filter.ts`
  - `repo-ref/xyflow/packages/system/src/xypanzoom/eventhandler.ts`

## Problem

The runtime owns `ViewportTransform`, `ViewportPanRequest`, `ViewportZoomRequest`, and
`ViewportMoveKind`, but it does not own the policy that decides whether a wheel, pinch, or pointer
gesture should become one of those requests. That makes the current module shallow for XyFlow feel:
the interface exposes math helpers, while important interaction invariants leak to adapters.

## Target State

The lane closes when Jellyflow has a renderer-neutral viewport gesture policy module that:

- accepts normalized adapter input rather than DOM, winit, egui, Fret, or wgpu events;
- resolves pan/zoom gates from `NodeGraphPanInteraction` and `NodeGraphZoomInteraction`;
- returns deterministic `ViewportPanRequest`, `ViewportZoomRequest`, or rejection reasons;
- records gesture kind decisions usable by conformance traces;
- preserves the existing renderer-free core/runtime crate contract.

## In Scope

- Headless viewport gesture input/value types.
- XyFlow-aligned gate behavior for pan-on-scroll, zoom-on-scroll, pinch zoom, pan-on-drag buttons,
  user selection active, connection in progress, and disabled interactions.
- Runtime tests that exercise the policy through the same public seam adapters should use.
- Adapter conformance scenario support for policy decisions when that can be done without renderer
  dependencies.

## Out Of Scope

- Browser, DOM, d3, winit, egui, Fret, or wgpu dependencies.
- Renderer screenshot, pixel, or event-loop tests.
- Pan inertia scheduling.
- Double-click zoom animation.
- Breaking the existing `ViewportPanRequest` and `ViewportZoomRequest` constructors.

## Starting Assumptions

| Assumption | Confidence | Evidence | Consequence if wrong |
| --- | --- | --- | --- |
| Adapters can normalize raw input before calling runtime policy. | High | Existing drag, auto-pan, and viewport request types are already headless. | The policy interface would need a lower-level input model or adapter-specific crates. |
| XyFlow pan/zoom gating is behavior, not rendering. | High | `repo-ref/xyflow/packages/system/src/xypanzoom/filter.ts` mostly decides enable/disable state and buttons. | Move more of this lane into adapter conformance fixtures only. |
| Core/runtime must remain renderer-free. | High | ADR-0001 and ADR-0003. | Stop and reopen the ADR before adding renderer/platform dependencies. |
| First slice can be proven with pure tests. | High | Existing viewport tests already cover math and gesture callback ordering. | Split conformance integration into a follow-on task. |

## Architecture Direction

Create a deep runtime module for viewport gesture policy. The public interface should be smaller
than the implementation: adapters provide normalized input plus current interaction views, and the
module returns an intent or a reasoned rejection. The module should depend on existing interaction
views instead of reading the entire interaction state where possible.

The seam is inside `jellyflow-runtime`, not in an adapter crate. Adapters remain concrete adapter
implementations for renderer/platform input normalization only.

## Closeout Condition

This lane can close when:

- the policy module is implemented and tested;
- conformance or runtime tests prove XyFlow-aligned gates;
- public surface tests cover any exported types;
- evidence gates pass;
- docs reflect the shipped behavior;
- and follow-on work is either split or explicitly deferred.
