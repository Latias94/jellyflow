---
type: "Decision"
title: "Node UI Kit Component Contract"
description: "Contract for custom node internals that stays headless while supporting rich adapter-local UI."
tags: ["engineering-memory", "jellyflow", "node-kit", "adapter", "custom-node-ui"]
timestamp: 2026-06-30T02:35:00+08:00
status: "active"
---

# Decision

Jellyflow should standardize rich custom nodes through a headless **Node UI Kit Component
Contract**: runtime owns semantic facts, measurements, typed feedback, and conformance; adapters own
toolkit widgets, local component libraries, focus, menus, and actual layout reporting.

This keeps the project on the `headless semantic surface + node kits + adapter-local mapping`
direction instead of building a shared widget crate.

# Contract

Headless runtime owns:

- node kit manifests, node schemas, default data, fixtures, and layout hints;
- semantic slots such as field rows, config groups, port rails, previews, badges, status banners,
  nested regions, action rows, and chrome descriptors;
- the `slot` vs `anchor` distinction: `slot` is data lookup, `anchor` is placement or port binding;
- typed ports, compatibility diagnostics, capacity, and connection lifecycle outcomes;
- `NodeInternalsController` as the adapter-facing path for invalidating, reporting, and querying
  node-internal geometry;
- measurement freshness through revisioned fresh/dirty/missing status, measured handles, measured
  surface slots, and measured anchors;
- conformance fixtures that prove Dify-style workflow, Shader/Blueprint, ERD, and mind-map shaped
  nodes keep stable anchors and feedback across adapters.

Adapters own:

- egui, GPUI, Dioxus, DOM, or self-drawn widget/component instances;
- local layout and paint code, including component-library widgets;
- focus, hover, popup, menu, inspector, and toolbar state;
- dropped-wire menus and node creation UI;
- actual widget bounds collection and reporting back as `NodeMeasurement`;
- visual density, clipping, degradation, screenshot/pixel tests, and platform event loops.

# Scenario Matrix

| Scenario | Required contract |
| --- | --- |
| Dify-style workflow | config groups, variable/tool/model controls, status chrome, validation banners, run/action strips, dropped-wire actions |
| Shader Graph / Blueprint | typed ports, exec/data separation, port rails, invalid hover feedback, preview regions, wire style tokens |
| ERD / data modeling | repeatable field rows, key badges, field anchors, resize-safe measured handles |
| Mind-map / knowledge canvas | low-zoom shell density, collapsible/nested regions, relation/source chips, keyboard-friendly editing |

# Current Proof Level

- egui has the strongest proof today: current-frame node region measurement reports measured handles,
  slots, anchors, and invalidates data, resize, zoom, and component-state changes.
- GPUI proves adapter-local component projection: rendering and runtime measurement now share
  `NodeSurfaceComponentLayout`, so slot and anchor rects are derived from the same local component
  model. This is still a projection-layout proof, not yet a true GPUI layout-pass bounds callback.
- `jellyflow-proof` proves component-tree shape plus runtime measurement integration, including
  dynamic child remeasurement. It intentionally avoids Dioxus or widget types.

# Next Contract Gaps

- repeatable slot/field collections for ERD fields, shader dynamic inputs, and Dify parameter lists;
- control descriptors for input/select/toggle/code/color/asset/variable-picker controls;
- action/menu descriptors for dropped-wire insert menus, node menus, graph menus, and inspector
  actions;
- diagnostics bound to slots, fields, ports, and chrome, not only commit-time edge errors;
- adapter capability matrix covering measured anchors, dynamic internals, typed feedback, menus,
  inspector, chrome, visual regression, and accessibility;
- a real GPUI layout-pass measurement hook before claiming full retained-view geometry parity.

# Roadmap

P0 should finish the adapter capability matrix and turn the current egui/GPUI/proof checks into
named conformance expectations. The minimum useful matrix columns are measured handles, measured
semantic anchors, dirty-to-fresh internals, typed hover feedback, typed commit rejection, chrome,
density degradation, dropped-wire menu, inspector, visual regression, and keyboard accessibility.

P1 should add repeatable fields and control descriptors. This is the layer needed for Dify
parameter lists, ERD tables with arbitrary fields, shader dynamic inputs, and Blueprint-style pins
without copying widget logic into runtime.

P1 should also add action/menu descriptors for dropped-wire insert menus, node menus, graph menus,
toolbar actions, and inspector actions. Runtime should describe the action target, availability,
diagnostics, and graph operation intent; adapters should render the actual menus.

P2 should promote GPUI from projection-layout proof to retained layout-pass measurement by
collecting actual component bounds from open-gpui canvas overlays or element layout callbacks and
reporting those bounds through `NodeInternalsController`.

P2 should add visual and interaction regression suites per product shape: Dify workflow,
Shader/Blueprint, ERD, and mind-map/knowledge canvas. These should cover full/compact/shell density,
resize, dynamic slot changes, invalid hover, reconnect, and dropped-wire creation.

# Consequences

- Jellyflow can grow toward Rust's XYFlow-style node library without becoming a backend workflow
  engine or shader compiler.
- Complex product nodes remain portable across egui, GPUI, Dioxus, DOM, and self-drawn adapters.
- Adapters can feel native and use their own component libraries, while conformance lives in
  headless facts and fixtures.
- Any future shared helper must start as semantic metadata or adapter-local utilities; promotion to
  a shared UI crate requires proven duplication across at least two real adapters.

# Citations

- [ADR 0008](../../../adr/0008-semantic-surface-and-framework-adapter-boundary.md)
- [ADR 0009](../../../adr/0009-node-kit-and-adapter-local-mapping-boundary.md)
- [Node UI Capability Parity Plan](../../../plans/2026-06-29-001-feat-node-ui-capability-parity-plan.md)
- [Runtime measurement contract](../../../../crates/jellyflow-runtime/src/runtime/measurement.rs)
- [egui bridge](../../../../crates/jellyflow-egui/src/bridge.rs)
- [jellyflow-proof tests](../../../../crates/jellyflow-proof/tests/proof.rs)
- [GPUI canvas-jellyflow proof](../../../../repo-ref/open-gpui/examples/canvas-jellyflow/src/main.rs)
