# Jellyflow Edge Path Module Split v1 - Design

Status: Closed
Date: 2026-06-01

## Problem

`runtime::geometry::paths` owns public renderer-neutral path types, straight path generation,
bezier control-point math, smoothstep-like routing, label placement, numeric helpers, and tests in
one file. That is still small enough to understand, but edge routing and label/hit-test behavior are
likely to grow as adapters such as egui, wgpu, and Fret consume the geometry substrate.

## Target State

- `runtime::geometry::paths::mod` is a private facade that preserves existing public re-exports from
  `runtime::geometry`.
- Public path types are separate from straight, bezier, smoothstep, and label helper logic.
- Existing renderer-neutral path tests keep passing.
- No path output, label placement, command ordering, or public API path changes.

## Scope

- Split `crates/jellyflow-runtime/src/runtime/geometry/paths.rs` into a directory module.
- Preserve `jellyflow_runtime::runtime::geometry::*` public path APIs.
- Keep edge command output, label offsets, smoothstep routing, and bezier curvature behavior
  unchanged.
- Update workstream evidence and closeout docs.

## Non-Goals

- No new edge routing algorithm.
- No adapter-specific path conversion for SVG, egui, Fret canvas, or wgpu.
- No hit-test behavior changes.
- No spatial-index backend.
- No renderer, platform, DOM, d3, or adapter dependencies.

## Architecture Direction

Use a facade plus owned submodules:

1. `mod.rs`: module declarations, public re-exports, and test module hook.
2. `types.rs`: `PathCommand`, `EdgePathLabel`, and `EdgePath`.
3. `straight.rs`: `straight_edge_path`.
4. `bezier.rs`: `BezierEdgeOptions`, `bezier_edge_path`, and bezier control helpers.
5. `smoothstep.rs`: `SmoothStepEdgeOptions`, `smoothstep_edge_path`, and orthogonal routing
   helpers.
6. `label.rs`: shared label placement helpers.
7. `tests.rs`: existing path behavior tests.

The split is behavior-preserving. If public-surface or path tests fail, preserve existing
re-exports and math exactly rather than changing call sites or expected output.

## Outcome

Closed on 2026-06-01. `runtime::geometry::paths` is now a private facade over focused `types`,
`straight`, `bezier`, `smoothstep`, `label`, and `tests` submodules. Public
`runtime::geometry::*` path APIs, path command output, label placement, and renderer-free geometry
boundaries remain unchanged.
