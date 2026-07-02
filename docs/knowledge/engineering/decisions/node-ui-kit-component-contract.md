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
  slots, anchors, and invalidates data, resize, zoom, and component-state changes. It now maps
  authoring controls, repeatable rows, node actions, dropped-wire menus, and inspector descriptors
  to adapter-local egui widgets or panels.
- `jellyflow-open-gpui` is now the first-class retained GPUI adapter boundary in the Jellyflow
  workspace. It owns GPUI adapter capability facts, view-space measurement conversion, authoring
  plans, scoped ids, product fixture gates, and widget-free graph affordance evidence without moving
  GPUI widget types into runtime/core.
- GPUI now has a first-class adapter crate plus an open-gpui consumer fixture. The adapter crate
  maps controls, repeatables, actions, menus, inspector plans, product fixture regression gates,
  graph affordance reports, and measurement conversion locally. The open-gpui `canvas-jellyflow`
  example consumes that boundary as the native product gallery and manual smoke surface.
- Open GPUI layout-pass evidence is coverage-gated. Live `measured_element` bounds can support
  layout-pass capability when all required regions are present and fresh; projection fallback
  remains the honest downgrade for initial, dirty, hidden, partial, duplicate, or missing bounds.
- The current Open GPUI product renderer foundation is host-local: `node_component_kit` owns
  adaptive layout stack primitives, component composition helpers, event shielding, and measured
  wrappers; runtime and `jellyflow-open-gpui` only see semantic descriptors, preset budgets, and
  report facts. The current mature Open GPUI path also gates native lifecycle, screenshot ROI,
  graph affordances, reconnect sequences, and component fit through widget-free evidence.
- `jellyflow-proof` proves component-tree shape plus runtime measurement integration, including
  dynamic child remeasurement. It intentionally avoids Dioxus or widget types.

# Completed Contract Additions

- field/control descriptors for input, textarea, select, slider, toggle, expression, code, color,
  asset, variable-picker, and port-binding semantics;
- repeatable collection descriptors with stable item identity, add/remove/reorder action keys, and
  item-id based anchors;
- action, menu, inspector, blackboard, and dropped-wire intent descriptors without adapter widget
  types;
- adapter capability reporting that distinguishes `none`, `projection`, `partial`, and `full`;
- egui geometry regression gates for Dify-style workflow, Shader/Blueprint, ERD, and mind-map
  product shapes;
- GPUI projection capability reporting that explicitly avoids claiming full layout-pass
  measurement;
- a `jellyflow-open-gpui` crate boundary for the retained GPUI adapter, with projection fallback
  versus layout-pass capability reporting, descriptor-driven controls/actions/inspector plans, and
  product fixture regression gates.
- graph affordance evidence for Open GPUI committed route family, preview-route parity, port
  placement budget, endpoint/reconnect hit budget, drag-region coverage, and readable layout-region
  coverage;
- host-local adaptive Open GPUI layout primitives for product renderers, including full/compact/shell
  degradation and repeatable overflow indicator budgeting.
- Open GPUI component fit evidence that records text/control/repeatable coverage, compact/shell
  degradation, required and present overflow indicators, clipped text/controls, and hidden
  repeatables without indicators.
- Open GPUI reconnect sequence evidence that covers source/target endpoint switching, edge-id
  preservation, invalid rollback, empty drops, and recovery after a rejected reconnect.

# Remaining Contract Gaps

- diagnostics bound to slots, fields, ports, and chrome, not only commit-time edge errors;
- adapter capability coverage for keyboard accessibility, focus order, screen-reader labels, and
  framework-specific widget behavior;
- pixel-golden visual regression for GPUI beyond screenshot smoke, ROI evidence, and structured
  geometry reports;
- broader adapter-native visual automation for GPUI and future Dioxus beyond deterministic fixture
  geometry.
- mature egui and Dioxus parity for the new Open GPUI canvas interaction foundation. The current
  stage intentionally validates shared contracts through egui/proof without promising equivalent
  graph-tool UX in those adapters.

# Roadmap

P2 should extract only the Open GPUI pieces that have survived real host use: generic canvas
interaction primitives belong in `repo-ref/open-gpui/crates/canvas`, widget-free adapter contracts
belong in `jellyflow-open-gpui`, and concrete node component composition should remain host-local
until more Open GPUI apps create reuse pressure.

P2 should deepen visual and interaction regression suites per adapter. egui has code-level geometry
gates plus a gallery snapshot path; GPUI now has structured product reports and screenshot smoke, but
still needs pixel/ROI review automation, accessibility/focus-order gates, and more native edge-label
or graph-toolbar coverage.

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
- [Open GPUI adapter crate](../../../../crates/jellyflow-open-gpui/src/lib.rs)
- [jellyflow-proof tests](../../../../crates/jellyflow-proof/tests/proof.rs)
- [GPUI canvas-jellyflow fixture](../../../../repo-ref/open-gpui/examples/canvas-jellyflow/src/main.rs)
