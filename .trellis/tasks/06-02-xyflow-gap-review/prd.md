# XyFlow gap review

## Goal

Produce a careful, evidence-backed review of how far Jellyflow is from XyFlow
behavioral parity. The output should be a durable gap report that distinguishes
three things:

1. XyFlow behavior Jellyflow already covers at the headless runtime boundary.
2. Real Jellyflow runtime/conformance gaps that should become follow-up Trellis
   tasks.
3. XyFlow React/DOM/renderer features that belong in future adapter crates, not
   in `jellyflow-core` or `jellyflow-runtime`.

The task is review-first. It should not implement parity changes.

## Confirmed Facts

- `repo-ref/xyflow` is the intended behavior reference source.
- XyFlow's reusable interaction substrate is concentrated in
  `repo-ref/xyflow/packages/system/src/`:
  - `xydrag`
  - `xyhandle`
  - `xypanzoom`
  - `xyresizer`
  - `utils`
  - `types`
- XyFlow's React package owns DOM integration, wrappers, hooks, renderers,
  controls, minimap, background, toolbars, providers, and store binding under
  `repo-ref/xyflow/packages/react/src/`.
- Jellyflow's current equivalent headless surfaces include:
  - `jellyflow-core`: graph model, typed IDs, validation, transactions,
    fragment/diff/history/normalize helpers.
  - `jellyflow-runtime`: store, rules, schema/profile, policy, selection,
    drag, resize, delete, keyboard, connection/reconnect, viewport, auto-pan,
    geometry, rendering helpers, `runtime::xyflow`, and conformance fixtures.
  - `templates/headless-adapter`: external adapter conformance smoke.
- ADR 0003 requires XyFlow feel to be tested through headless contracts first;
  renderer smoke tests belong in future adapter crates.
- `CONTEXT.md` already identifies likely follow-on lanes: pointer-resize parity,
  nested parent semantics, selection-specific auto-pan, visible edge culling,
  full scene render plans, real spatial indexing, async pre-delete, renderer
  smoke harnesses, and schema migration only after evidence.

## Requirements

- Review XyFlow and Jellyflow source with file-level evidence.
- Create `docs/reviews/xyflow-gap-2026-06-02.md`.
- Cover at least these comparison areas:
  - model and store semantics
  - node/edge changes and controlled-mode projection
  - connection, reconnection, handles, validation, and connection radius
  - selection, keyboard intent, and delete behavior
  - node dragging, extents, parent expansion, snapping, and keyboard movement
  - node resizing, parent/child clamps, keep-aspect-ratio, and resize callbacks
  - viewport pan/zoom, fitView, translate extent, pan-on-scroll, double-click,
    animation, and inertia
  - auto-pan for node drag, connect, selection, and node focus
  - geometry, edge paths, hit testing, visible nodes, visible edges, and render
    order
  - React/DOM-only features such as wrappers, providers, controls, minimap,
    background, toolbar, portals, accessibility text, and DOM measurement
- Classify each area as one of:
  - `covered`
  - `partial`
  - `missing`
  - `adapter-owned`
  - `intentionally out of scope`
- For each meaningful gap, provide:
  - source evidence from XyFlow
  - current Jellyflow evidence
  - why the gap matters
  - whether the gap belongs in core, runtime, conformance, template, docs, or
    future adapter crates
  - recommended priority
  - suggested follow-up Trellis task title
- Preserve accepted boundaries from ADR 0001 and ADR 0003. Do not recommend
  moving DOM, React, renderer, screenshot, or pixel responsibilities into the
  headless crates.
- Avoid implementation changes during this task.

## Acceptance Criteria

- [x] A review report exists at `docs/reviews/xyflow-gap-2026-06-02.md`.
- [x] The report includes a comparison matrix with coverage classifications and
      file-level evidence.
- [x] The report lists top parity gaps in priority order.
- [x] The report separates runtime/conformance gaps from adapter-owned React/DOM
      responsibilities.
- [x] The report explains why some XyFlow features should not be implemented in
      headless crates.
- [x] The report proposes concrete follow-up Trellis task candidates.
- [x] No runtime/core behavior changes are made in this task.
- [x] Validation evidence is recorded in the task.

## Out Of Scope

- Implementing any parity gap.
- Adding renderer, DOM, React, wgpu, egui, screenshot, or pixel tests to the
  headless crates.
- Schema migration.
- Release/publishing work.
- Recreating the old workstream system.

## Resolved Scope Decision

XyFlow React UI components are adapter-owned inventory for this review, not
first-class Jellyflow headless parity targets. The report should prioritize
`packages/system` interaction, geometry, viewport, resize, and connection
semantics while recording React UI responsibilities for future adapter planning.
