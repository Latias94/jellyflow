# Jellyflow Visible Render Order Contract v1

Status: Active
Last updated: 2026-06-02

## Why This Lane Exists

Jellyflow now exposes two renderer-neutral pieces adapters need before painting nodes:
`NodeGraphStore::node_render_order()` and `NodeGraphStore::visible_node_ids(viewport_size)`.
Keeping them as separate adapter calls leaks runtime composition rules into every consumer:

- whether culling is enabled;
- how hidden nodes are filtered;
- how viewport size and transform affect visibility;
- how explicit draw order and selected-node elevation combine with visibility.

For an XyFlow-like feel, adapters should not reimplement that composition. They should be able to
ask the headless runtime for the ordered list of currently paintable node ids and then render those
ids in order. This deepens `runtime::rendering`: the Interface becomes smaller while the
Implementation owns more of the behavior.

## Target State

- A renderer-neutral runtime helper resolves visible node render order from a graph, lookups, view
  state, viewport request, and render-order options.
- `NodeGraphStore` exposes a public helper that reads current view state and resolved runtime
  tuning, including `only_render_visible_elements` and selected-node elevation.
- The helper filters visible ids through the existing node render order, so adapters get
  deterministic draw order with selected nodes elevated after non-selected visible nodes.
- The contract remains headless and renderer-free; renderer crates may consume it later without
  changing runtime semantics.
- Conformance/template coverage can assert ordered visible node ids before any screenshot or pixel
  harness exists.

## Scope

In scope:

- node ids only, ordered for rendering;
- current viewport pan/zoom and logical viewport size;
- hidden node filtering, partial visibility, disabled culling, and selected-node elevation;
- runtime/store tests and public surface smoke;
- conformance action and template smoke coverage for adapter-facing ordered visible node ids.

Out of scope:

- visible edge culling and edge path/AABB semantics;
- group, sticky note, edge, or full scene render plans;
- real spatial indexing, grids, R-trees, quadtrees, or performance tuning;
- `wgpu`, egui, Fret, screenshots, pixels, or renderer smoke harnesses;
- compatibility shims for adapters that already compose the two older helper calls manually.

## Source Coverage

| Source | State | Evidence | Impact |
| --- | --- | --- | --- |
| User goal | COVERED | ongoing `$improve-codebase-architecture` + `$fearless-refactor` goal | Continue headless XyFlow-feel contracts while keeping runtime renderer-free. |
| Repo context | COVERED | `CONTEXT.md` | Runtime rendering and visible-node contracts are documented; visible edge culling and spatial indexing are follow-ons. |
| ADRs | COVERED | ADR 0001, ADR 0003 | Headless crates own behavior contracts; renderer smoke belongs in adapter crates. |
| Prior workstream | COVERED | `jellyflow-visible-elements-contract-v1` | `visible_node_ids` is available and should be composed inside the runtime Module, not every Adapter. |
| Jellyflow code | COVERED | `crates/jellyflow-runtime/src/runtime/rendering.rs`, rendering tests, conformance action runner, `templates/headless-adapter` | Existing order and visibility helpers can be deepened into one adapter-facing seam. |
| Architecture lane map | MISSING | no `docs/architecture/` directory | Not blocking; this workstream is its own authoritative lane artifact. |
| Renderer harness | OUT_OF_SCOPE | ADR 0003 | Runtime contract should land before optional `jellyflow-wgpu` or adapter pixel tests. |

## Architecture Direction

The new seam should be a small headless Interface:

- pure runtime function for tests and future backend replacement;
- store helper for ordinary adapters;
- conformance action for fixture-level assertions.

The Implementation should reuse `resolve_node_render_order` and `resolve_visible_node_ids` rather
than duplicating ordering or culling logic. If the visible-node backend later becomes indexed, this
contract should keep working unchanged.

## Task Plan

- VRO-010 opens the workstream and freezes source coverage.
- VRO-020 adds the runtime/store visible render order contract and focused tests.
- VRO-030 adds conformance/template coverage for adapter-facing ordered visible node assertions.
- VRO-040 updates docs, records closeout evidence, and splits broader render-plan follow-ons.

## Closeout Condition

This lane can close when:

- adapters have a public headless helper for ordered visible node ids;
- store helper behavior respects `only_render_visible_elements` and selected-node elevation;
- conformance/template smoke can assert ordered visible node ids;
- docs explain why full scene render plans, visible edge culling, and renderer harnesses remain
  follow-ons;
- focused runtime, template, package, JSON, and diff gates pass.
