# jellyflow-open-gpui

First-class Open GPUI adapter boundary for Jellyflow.

The crate maps Jellyflow's headless semantic node contracts to Open GPUI
adapter plans, authoring decisions, stable ids, measurement facts, and product
fixture gates. Runtime and core crates stay toolkit-free. Concrete Open GPUI
widgets, focus/popup lifecycle, app state refresh, demo item factories, and
visual layout remain host-owned.

Current capability reporting is deliberately conservative. Projection fallback
can prove controls, repeatables, menus, inspector state, and product fixture
geometry, but full layout-pass measurement is only valid when coverage shows
the required regions came from Open GPUI `measured_element` bounds without
fallback, missing, stale, partial, or duplicate regions.

## Authoring Facade

`jellyflow-open-gpui` owns reusable adapter infrastructure that Open GPUI hosts
should not copy from `canvas-jellyflow`:

- JSON binding rules for control, inspector, and repeatable edits live in one
  adapter-local module. Dot paths, JSON pointers, `Slot` lookup, field-row
  fallback, scalar repeatable item ids, and non-writable graph/port controls now
  share the same behavior.
- `OpenGpuiAuthoringController` plans control events from the current
  `NodeGraphStore`, so fast consecutive edits do not rely on stale node-data
  snapshots captured by a widget callback.
- Semantic action dispatch can map repeatable add/remove/reorder intents into
  `OpenGpuiRepeatableActionPlan` values, with explicit skipped outcomes for
  unsupported actions and a host-supplied factory for rich add-item defaults.
- `element_ids` exports stable scoped ids for controls, menus, action buttons,
  slot badges/values, repeatable rows/actions, blackboard rows/status, and
  fallback chrome. Node ids are part of node-internal ids; graph-level actions
  use the graph scope.
- `OpenGpuiNodeRendererRegistry::render_with_host` passes custom renderers a
  generic `OpenGpuiNodeRendererHostContext` containing semantic slots,
  repeatables, action menus, measurement id helpers, scoped id helpers, fallback
  reasons, and host-owned services. The output type remains generic, so this
  crate does not expose Open GPUI element types upstream.

`canvas-jellyflow` is still the live host fixture. It owns `Button`, `Menu`,
`TextInput`, `Textarea`, `Select`, `Switch`, `Slider`, `measured_element`,
weak-entity dispatch, focus/event arbitration, refresh notifications, and demo
visual policy. That split is intentional: Jellyflow provides the semantic and
adapter authoring contract, while each Open GPUI app maps the contract into its
own component tree.

## Node Component Kit Recipe

Open GPUI node-internal UI is now proven through a host-local component kit in
`repo-ref/open-gpui/examples/canvas-jellyflow/src/node_component_kit.rs`.
That module is intentionally not a shared widget crate. It is the reference
consumer for Open GPUI apps that want Dify-style cards, shader/material rows,
ERD fields, or mind-map previews while keeping runtime descriptors headless.

The minimal recipe is:

1. Define the semantic node in runtime or a node kit. The descriptor should
   publish slots, controls, repeatables, actions, anchors, and a renderer key.
2. Register the renderer key with `OpenGpuiNodeRendererRegistry::with_renderer`
   or `with_renderers`. In `canvas-jellyflow`, `demo_node_renderer_registry`
   declares `decision-card`, `shader-card`, `table-card`, `topic-card`, and
   `source-card`.
3. Keep concrete Open GPUI renderers host-local. The example stores renderer
   closures in `demo_custom_node_renderers` and resolves them through
   `OpenGpuiNodeRendererRegistry::render_with_host`.
4. In the renderer, read semantic data from
   `OpenGpuiNodeRendererHostContext::semantic()` and compose real Open GPUI
   elements through the local kit:
   `render_control_plan`, `render_action_menu`,
   `render_dispatch_action_button`, `repeatable_action_button`,
   `render_node_internal_interaction_region`, and `render_measured_region`.
5. Route edits through adapter plans instead of mutating captured widget state.
   Controls use `OpenGpuiAuthoringController` against the live
   `NodeGraphStore`; repeatables use `OpenGpuiRepeatableActionPlan` with
   explicit host factories for product defaults.
6. Wrap every visible slot, control, repeatable row, and handle anchor that can
   drive ports, inspectors, or edge endpoints in `render_measured_region`.
   Missing, stale, hidden, partial, or duplicate regions must downgrade the
   capability report rather than claiming full layout-pass coverage.

Native controls currently include text input, textarea, number input, select,
multiselect-as-select, switch, and slider. Code editor and color controls render
as partial badge states; asset picker, variable picker, and port-binding picker
render as disabled stub/display states until a later Open GPUI product pass.

## Layout-Pass Measurement

`canvas-jellyflow` is the live Open GPUI consumer. It renders node internals
from Jellyflow semantic descriptors, wraps visible slots, controls, repeatable
items, and anchors with Open GPUI `measured_element`, and reports those bounds
into `OpenGpuiBoundsCollector` using stable `OpenGpuiMeasurementId` values.

`jellyflow-open-gpui` owns the reusable conversion contract:

- `OpenGpuiMeasurementId` creates descriptor-backed ids for slots, scoped
  controls, repeatable items, and anchors.
- `OpenGpuiMeasurementContext` converts GPUI view-space bounds into
  node-local `NodeMeasurement` slots and anchors.
- `layout_pass_measurement_from_regions` merges live layout-pass regions with
  explicit projection fallback anchors and returns `OpenGpuiMeasurementCoverage`.
- `OpenGpuiAdapter::layout_pass(coverage)` can only report full layout-pass
  support when coverage has layout-pass regions and no projection fallback,
  missing, stale, partial, or duplicate regions.

Projection remains a first-class degrade path. When live bounds are missing,
dirty after a node-internal edit, duplicated, zero-sized, or hidden by density,
the adapter must report projection/partial coverage instead of presenting the
data as full layout-pass support. Runtime/core/layout crates remain toolkit-free;
Open GPUI-specific widgets and measurement plumbing stay in this crate and the
`repo-ref/open-gpui/examples/canvas-jellyflow` consumer fixture.

## Graph Affordance Evidence

Open GPUI graph interaction polish is also reported through widget-free evidence.
`OpenGpuiGraphAffordanceEvidence` records:

- committed wire route family (`Orthogonal` or `Bezier` for product routes);
- whether connection previews mirror committed route policy instead of falling
  back to direct lines;
- port placement, endpoint hit, and reconnect affordance budgets;
- drag-region coverage for product nodes;
- readable layout-region coverage for node-internal UI.

`assert_product_interaction_report_gates` fails if this evidence is missing, if
previews use `DirectLineFallback`, if port/reconnect budgets are too small, or if
hidden repeatable rows have no visible overflow indicator. The concrete canvas
implementation still lives in Open GPUI: route geometry, hit testing, pointer
capture, reconnect handles, and connection release events belong to
`repo-ref/open-gpui/crates/canvas`. This crate only defines the Jellyflow adapter
contract and report gates.

The host-local product renderer layer may use layout primitives from
`repo-ref/open-gpui/examples/canvas-jellyflow/src/node_component_kit.rs`, but
those primitives are product component code. They can decide how Dify, shader,
ERD, topic, and source cards degrade, clamp, or reveal overflow, while the
adapter only consumes explicit measured internals and component-declared
overflow. Jellyflow does not expose a shared cross-framework widget crate or an
arbitrary text/control fit oracle.

## Native UX Evidence

Native polish is gated with structured reports before screenshots or manual
review are considered. `OpenGpuiNativeLifecycleEvidence` proves that the product
gallery rendered real Jellyflow content, that product node dragging was checked,
and that the closest available Open GPUI last-window-close path observes quit.
Skipped close automation and blank-window smokes fail the hard gate.

`OpenGpuiMeasuredInternalsEvidence` is the hard visual proof for product node
internals. It records node-bound source, handle coverage, readable-region
coverage, drag-exclusion coverage, stale regions, and component-declared
overflow. The visual gate rejects missing or incomplete measured internals,
projection fallback used as hard proof, hidden repeatables without explicit
overflow declaration, stale regions, and endpoints that do not follow measured
handles.

`OpenGpuiReconnectSequenceEvidence` proves selected-edge reconnect is actionable,
not only visible. The product interaction gate requires source and target endpoint
switches, edge-id preservation, invalid rollback, empty reconnect drop reporting,
and a second gesture after rejection without stale planning noise.

## Product Gates

The test helpers in `jellyflow_open_gpui::testing` cover Dify-style workflow
nodes, shader/blueprint repeatables, ERD field rows, and mind-map shells. They
keep projection fixture coverage separate from layout-pass fixture coverage, so
a projection-only report cannot satisfy full layout-pass capability. The same
helpers now expose structured authoring interaction evidence for dropped-wire
insert actions, inspector target sources, repeatable add/remove/reorder/edit,
blackboard actions, invalid hover rejection, editable control regions, scoped
element ids, renderer host-context routing, product-gallery host rendering,
visual interaction geometry, and dynamic repeatable port lifecycle honesty.
They also require graph affordance evidence for product route/preview policy,
port and reconnect hit budgets, drag-region coverage, and readable adaptive
layout coverage.

The Open GPUI gallery lives in `repo-ref/open-gpui/examples/canvas-jellyflow`.
Run it with:

```sh
cargo run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --features open_gpui_platform/runtime_shaders --bin open-gpui-canvas-jellyflow
```

The hard structured gate is:

```sh
cargo nextest run -p jellyflow-open-gpui --no-fail-fast
cargo test --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --features open_gpui_platform/runtime_shaders --bin open-gpui-canvas-jellyflow -- --nocapture --test-threads=1
```

The gallery also has a test-only screenshot smoke exporter. When the platform
has a headless renderer, the bin tests write nonblank PNG review artifacts under
`repo-ref/open-gpui/target/open-gpui-jellyflow-gallery/`. These screenshots are
supporting evidence only; structured geometry, capability, and interaction
reports remain the correctness gate.
