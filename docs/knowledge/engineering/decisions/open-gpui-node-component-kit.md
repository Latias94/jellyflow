---
type: "Decision"
title: "Open GPUI Node Component Kit"
description: "Where the concrete Open GPUI node component kit lives and how it relates to Jellyflow's headless contracts."
tags: ["open-gpui", "node-ui", "component-kit", "adapter-boundary"]
timestamp: 2026-07-01T22:01:10+08:00
related_plan: "../../../plans/2026-07-01-003-feat-open-gpui-node-component-kit-plan.md"
---

# Decision

The Open GPUI node component kit is a host-local kit under
`repo-ref/open-gpui/examples/canvas-jellyflow/src/node_component_kit.rs`, backed by widget-free
adapter contracts in `jellyflow-open-gpui`.

Jellyflow does not add a shared widget crate or runtime-owned widget model in this stage. Runtime
and node kits publish semantic descriptors: slots, anchors, controls, repeatables, actions, menus,
diagnostics, renderer keys, and measurement ids. `jellyflow-open-gpui` translates those descriptors
into adapter plans, scoped ids, renderer host contexts, authoring outcomes, and structured reports.
The Open GPUI host maps those plans into concrete components, focus behavior, popup/menu state,
event shielding, measured regions, and product renderer composition.

The current canvas/node-UI foundation adds one more boundary:

- `repo-ref/open-gpui/crates/canvas` owns generic graph UX primitives such as route-aware wires,
  preview route policy, pointer capture, drag threshold behavior, connection release events,
  reconnect handles, invalid-target feedback, and hit-test geometry. It must not depend on
  Jellyflow.
- `jellyflow-open-gpui` owns widget-free Open GPUI evidence and presets: surface style budgets,
  readable sizes, component fit budgets, graph affordance evidence, native lifecycle evidence,
  product interaction reports, and regression gates.
- `canvas-jellyflow` owns concrete Open GPUI components and host-local layout primitives:
  `AdaptiveNodeLayoutStack`, repeatable overflow planning, renderer-specific component placement,
  focus/menu state, weak-entity dispatch, and native product gallery composition.

# Context

The target experience is Rust-native custom node internals: Dify-style workflow cards, shader or
blueprint nodes, ERD table editors, and mind-map/knowledge canvas topics. These experiences need
real controls and rich layouts, but the same semantic graph should remain portable to egui, Dioxus,
proof/template adapters, and future hosts.

Open GPUI is the first mature adapter target because the local fork now exposes the component and
test hooks needed for a native retained UI proof. The product gallery uses host renderer keys
`decision-card`, `shader-card`, `table-card`, `topic-card`, and `source-card`.

This decision does not claim mature egui or Dioxus parity for the new graph interaction layer.
Those adapters continue to validate the shared headless contracts, while the Open GPUI path is the
only mature native product surface in this stage.

# Alternatives

- Put widgets in runtime: rejected because it would couple headless graph semantics to one retained
  UI lifecycle and break the xyflow-like adapter boundary.
- Create `jellyflow-ui-widgets`: rejected for now because GPUI, egui, and Dioxus do not share
  widget lifecycles, event models, focus/popup behavior, or measurement APIs.
- Keep every GPUI renderer handwritten with no kit: rejected because product nodes repeated
  control rows, action dispatch, repeatable row, measured-region, and event-shielding code.
- Promote the kit into a public Open GPUI crate immediately: deferred until the host-local module
  survives more product renderers and external app use.

# Consequences

- Custom Open GPUI nodes follow a short recipe: publish a semantic descriptor with a renderer key,
  register the key in `OpenGpuiNodeRendererRegistry`, provide a host-local renderer closure, compose
  concrete elements through `node_component_kit`, route edits through adapter authoring plans, and
  wrap visible semantic regions with measured-element ids.
- Advanced controls remain explicit partial/stub states until productized. Code editor and color
  render as partial display badges; asset picker, variable picker, and port-binding picker render as
  disabled stubs/display states.
- Dynamic repeatable ports remain policy-driven. Adding an item must either create/update graph port
  facts or report `MissingGraphPort`; renderers must not publish fake fresh handles.
- Structured reports are the release gate. Screenshots are optional smoke/review artifacts under
  `repo-ref/open-gpui/target/open-gpui-jellyflow-gallery/`, not pixel-golden correctness oracles.
- Product graph affordances are now explicit report data. A host must prove non-straight committed
  route policy where applicable, connection previews that mirror committed route policy, port and
  reconnect hit budgets, drag-region coverage, readable layout-region coverage, and repeatable
  overflow indicators.
- Product node internals are also explicit report data. `OpenGpuiProductSurfacePreset` publishes a
  widget-free `component_fit_budget`, while the Open GPUI host maps it into local component layout
  and reports `OpenGpuiComponentFitEvidence` for text/control/repeatable fit, compact/shell
  degradation, required overflow indicators, clipping, and hidden repeatable overflow.
- Product renderers should use adaptive host-local layout plans rather than fixed absolute rows.
  Full density, compact density, and shell fallback are Open GPUI rendering decisions; runtime only
  publishes semantic budgets and overflow intent.

# Citations

- [Open GPUI Node Component Kit and Product Gallery Plan](../../../plans/2026-07-01-003-feat-open-gpui-node-component-kit-plan.md)
- [Node UI Kit Component Contract](node-ui-kit-component-contract.md)
- [jellyflow-open-gpui README](../../../../crates/jellyflow-open-gpui/README.md)
- [canvas-jellyflow node component kit](../../../../repo-ref/open-gpui/examples/canvas-jellyflow/src/node_component_kit.rs)
- [canvas-jellyflow product renderers](../../../../repo-ref/open-gpui/examples/canvas-jellyflow/src/product_renderers.rs)
- [Open GPUI Canvas Node UI Foundations Plan](../../../plans/2026-07-02-003-feat-open-gpui-canvas-node-ui-foundations-plan.md)
