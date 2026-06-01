# Jellyflow Node Drag Kernel v1 - Design

Status: Closed
Date: 2026-06-01

## Problem

Jellyflow now has renderer-neutral geometry, selection-box helpers, and a private interaction
harness, but node dragging still lacks a headless runtime kernel. Adapters for Fret, egui, wgpu, or
other self-drawn Rust shells would otherwise need to reimplement the same XyFlow-like drag rules:

- which nodes enter a drag session;
- how selected nodes move together;
- how `draggable`, hidden nodes, parent/child selection, snap grid, node origin, and extents apply;
- which graph transaction is committed;
- which runtime gesture and XyFlow compatibility callbacks fire in what order.

This lane turns node drag from renderer-specific event handling into a runtime behavior contract.

## Target State

Add a renderer-neutral node drag kernel that adapters can drive with canvas-space pointer intent:

- build deterministic drag items from a primary node and current selection;
- apply draggable policy and hidden-node filtering;
- preserve XyFlow's selected-parent semantics for child nodes;
- compute ordered `SetNodePos` transactions for drag updates;
- support snap-to-grid and global/per-node movement extents;
- record drag start/update/end behavior through the interaction harness;
- project XyFlow-style node changes and drag callbacks where applicable.

The output should be plain Rust data and normal `NodeGraphStore` transactions. Renderer adapters own
pointer capture, DOM/class filtering, mouse/touch quirks, screenshots, and pixels.

## Scope

- Add a `runtime::drag` kernel if the first implementation slice proves the API shape.
- Add harness-backed runtime tests for drag item selection, graph transactions, event ordering, and
  callback projections.
- Reuse existing `NodeGraphInteractionState`, policy resolution, geometry utilities, and
  `GraphOp::SetNodePos`.
- Keep the lane renderer-free and dependency-free.
- Update README/workstream evidence at closeout.

## Non-Goals

- Do not add `wgpu`, `winit`, egui, Fret UI, d3, DOM, screenshot, or pixel-test dependencies.
- Do not implement pan/zoom, resize, reconnect, or selection-box behavior in this lane.
- Do not implement browser-specific no-drag selectors, drag handles, mouse/touch capture, or
  multitouch abort behavior inside the runtime crate.
- Do not move persisted layout or policy fields out of `Graph`.
- Do not publish a standalone public fixture crate yet.

## Architecture Direction

The drag kernel should sit between adapter input and graph transactions:

```text
canvas-space pointer intent -> runtime::drag kernel -> GraphTransaction -> NodeGraphStore trace
```

The runtime should not know about windows, DOM nodes, d3, or rendering. Adapters provide already
normalized canvas-space points and decide when a drag starts based on their input substrate.

## Source Coverage

| Source | State | Evidence | Impact |
| --- | --- | --- | --- |
| User goal | COVERED | Request for XyFlow feel in a headless Rust node library | Node drag is the next interaction kernel. |
| ADR 0003 | COVERED | `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md` | Keeps tests renderer-free and fixture-oriented. |
| ADR 0002 | COVERED | `docs/adr/0002-jellyflow-model-policy-boundary.md` | Prevents schema movement while implementing drag policy. |
| Interaction harness lane | COVERED | `docs/workstreams/jellyflow-interaction-harness-v1/HANDOFF.md` | Provides the test harness and follow-on decision. |
| Geometry/spatial lane | COVERED | `docs/workstreams/jellyflow-geometry-spatial-v1/HANDOFF.md` | Supplies bounds, node origin, path, and hit-test primitives. |
| XyFlow drag reference | COVERED | `repo-ref/xyflow/packages/system/src/xydrag/` | Defines selected-parent filtering, snap offset, and drag event ordering. |
| Renderer adapter crates | OUT_OF_SCOPE | Future adapter workstreams | Renderer smoke tests belong outside runtime. |

## Risk Notes

- A too-large first API can freeze weak names. Start with transaction planning and tests before
  broadening public surface.
- Parent extent and expand-parent behavior can become subtle. Gate them with focused fixtures before
  claiming parity.
- Auto-pan requires viewport mutation plus drag updates. Keep it out of the first node-drag kernel
  unless a later task proves a clean headless contract.

## Closeout Summary

Closed on 2026-06-01. The renderer-neutral drag kernel now exposes deterministic drag planning,
normal `SetNodePos` transactions, selected-node co-dragging, snap-to-grid, global/per-node extent
clamping, node-origin-aware bounds math, node drag gesture payloads, and XyFlow-compatible callback
dispatch. Renderer input capture, parent expansion, auto-pan, public fixture format, and renderer
smoke tests remain follow-ons.
