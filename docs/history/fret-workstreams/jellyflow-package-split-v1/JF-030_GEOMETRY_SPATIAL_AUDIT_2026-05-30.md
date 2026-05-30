# JF-030 Geometry / Spatial Audit

Status: Complete
Last updated: 2026-05-30

## Decision

Do not extract `jellyflow-geometry` yet.

`CanvasGeometry`, `CanvasSpatialDerived`, route math, and hit-test helpers stay in `fret-node`.
The only reusable headless math seam identified in this audit is `jellyflow-runtime/src/runtime/fit_view.rs`,
which already lives in the runtime crate and does not justify a new geometry package by itself.

## Why

- `ecosystem/fret-node/src/ui/canvas/geometry/mod.rs` depends on `NodeGraphPresenter`,
  `NodeGraphStyle`, `NodeGraphGeometryOverrides`, and `NodeGraphNodeOrigin`.
- `ecosystem/fret-node/src/ui/canvas/spatial.rs` builds on `CanvasGeometry` and `fret_canvas`
  spatial primitives, so it is still adapter-side acceleration code.
- `ecosystem/fret-node/src/ui/canvas/route_math.rs` is small pure math, but its only consumer is
  the adapter canvas path.
- `ecosystem/fret-node/src/ui/declarative/paint_only/cache.rs` ties geometry and spatial caching to
  style fingerprints, override revisions, and paint-only invalidation keys.
- `ecosystem/jellyflow-runtime/src/runtime/fit_view.rs` is already headless-safe and reusable.
- `ecosystem/jellyflow-runtime/src/runtime/utils.rs` is graph-query sugar around lookups, not a
  reusable geometry substrate.

## Follow-up

1. Keep geometry, spatial, route math, and hit-test helpers in `fret-node`.
2. If a second non-adapter consumer appears, extract only the smallest pure math subset first.
3. Add a dedicated boundary gate before any future move that crosses `NodeGraphStyle`,
   `NodeGraphPresenter`, or `CanvasSpatialDerived`.

## Evidence Anchors

- `ecosystem/fret-node/src/ui/canvas/mod.rs`
- `ecosystem/fret-node/src/ui/canvas/geometry/mod.rs`
- `ecosystem/fret-node/src/ui/canvas/spatial.rs`
- `ecosystem/fret-node/src/ui/canvas/route_math.rs`
- `ecosystem/fret-node/src/ui/declarative/paint_only/cache.rs`
- `ecosystem/jellyflow-runtime/src/runtime/fit_view.rs`
- `ecosystem/jellyflow-runtime/src/runtime/utils.rs`
