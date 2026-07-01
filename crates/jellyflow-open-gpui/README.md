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

## Product Gates

The test helpers in `jellyflow_open_gpui::testing` cover Dify-style workflow
nodes, shader/blueprint repeatables, ERD field rows, and mind-map shells. They
keep projection fixture coverage separate from layout-pass fixture coverage, so
a projection-only report cannot satisfy full layout-pass capability. The same
helpers now expose structured authoring interaction evidence for dropped-wire
insert actions, inspector target sources, repeatable add/remove/reorder/edit,
blackboard actions, invalid hover rejection, editable control regions, scoped
element ids, and renderer host-context routing.
