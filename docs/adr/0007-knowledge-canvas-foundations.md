# ADR 0007: Knowledge Canvas Foundations

Status: Accepted
Date: 2026-06-12

## Context

Jellyflow now has a renderer-free graph model, runtime interaction layer, and optional layout crate.
That is enough for XyFlow-style node canvases and native mind-map layout, but knowledge-canvas
products need one more semantic layer. Nodes may represent excerpts, annotations, source regions,
or references into documents that the host product owns. Keeping those relationships only in
`Node.data` would make them hard to query, diff, copy, validate, and undo.

The same product pressure also affects layout and read queries. Mind-map engines should be
discoverable as a family without replacing stable engine IDs, and large canvases need a query seam
before any spatial backend can safely replace today's deterministic linear scans.

## Decision

Add first-class graph bindings as a portable core resource. A binding connects two endpoints:

- a graph-local target that Jellyflow can validate, such as a node, port, edge, group, sticky note,
  or the graph itself;
- an opaque host-owned source anchor with a stable source id and arbitrary payload.

`Node.origin` remains the node-rectangle origin used by runtime geometry. Binding anchors are a
separate knowledge-canvas relationship model and must not reuse `Node.origin` vocabulary or storage.

Core owns binding persistence, structural validation, transactions, diffing, undo/redo, and fragment
copy semantics. Runtime owns resolved binding facts: it may combine bindings with measurements,
node-origin fallback, visibility, and geometry to produce adapter-facing anchor facts. Layout engines
must continue to consume explicit `LayoutContext` values rather than reading `NodeGraphStore`.

Layout family metadata is a discovery layer over stable engine IDs. Built-in radial and freeform
mind-map engines can appear under a `mind_map` family while direct engine ID dispatch remains the
canonical execution contract.

Runtime query composition should move behind a read model that preserves current store method names.
The first backend must be behavior-equivalent to current linear scans; any spatial backend remains
optional behind tuning and must prove equivalent public results.

## Consequences

- Knowledge-canvas relationships become serializable, diffable, undoable graph facts.
- Core stays free of PDF, image, OCR, renderer, DOM, React, Fret, `wgpu`, `winit`, and egui
  dependencies.
- Host adapters keep ownership of source resource parsing, source lifecycle, and renderer overlays.
- Runtime can expose binding queries without forcing adapters to duplicate geometry rules or lookup
  internals.
- Layout family discovery can improve host UX without creating a second dispatch protocol.
- Query optimization has a stable behavioral seam before indexing is introduced.

## Follow-Up

- Add binding model types, serde-defaulted graph storage, and structural validation.
- Add reversible binding operations across apply, invert, normalize, diff, planner, and fragment
  copy.
- Add runtime binding queries and binding-derived layout context helpers.
- Add layout family metadata for existing built-in engines.
- Extract rendering, layout-facts, and binding reads into a runtime query module.
- Add optional spatial query backend only after equivalence tests exist.

## Evidence

- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/adr/0002-jellyflow-model-policy-boundary.md`
- `docs/adr/0005-layout-engine-extension-boundary.md`
- `docs/adr/0006-mind-map-layout-strategy.md`
- `crates/jellyflow-core/src/core/model/graph.rs`
- `crates/jellyflow-runtime/src/runtime/layout.rs`
- `crates/jellyflow-runtime/src/runtime/rendering/query.rs`
