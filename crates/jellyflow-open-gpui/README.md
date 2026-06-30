# jellyflow-open-gpui

First-class Open GPUI adapter boundary for Jellyflow.

The crate maps Jellyflow's headless semantic node contracts to Open GPUI local
components and measurement facts. Runtime and core crates stay toolkit-free;
this crate owns GPUI-specific widgets, focus, menus, inspector plans, product
fixture gates, and layout reporting.

Current capability reporting is deliberately conservative. Projection fallback
can prove controls, repeatables, menus, inspector state, and product fixture
geometry, but full layout-pass measurement is only valid when bounds come from
the Open GPUI element-bounds hook.

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
a projection-only report cannot satisfy full layout-pass capability.
