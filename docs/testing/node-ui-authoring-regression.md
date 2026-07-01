# Node UI Authoring Regression Gates

Jellyflow's rich-node regression gates are split between headless contract tests, adapter geometry
tests, and local visual review. Pixel snapshots are useful for review, but the hard gate is geometry
and semantic capability evidence that does not depend on GPU output.

## Product Shape Matrix

| Shape | Fixture | Required states | Hard evidence |
| --- | --- | --- | --- |
| Dify-style workflow | `SampleGraphKind::AutomationBuilder`, builtin `workflow.automation`, GPUI `demo.llm` schema probe | full, compact, shell review, resize, dropped-wire menu, inspector/action descriptors | `product_shape_snapshots_keep_authoring_regions_inside_nodes`, `density_modes_have_regression_coverage_for_rich_nodes`, `dropped_wire_menu_is_backed_by_authoring_action_descriptors`, `authoring_interaction_states_have_regression_fixtures`, `jellyflow-open-gpui::testing::product_fixtures_cover_gpui_authoring_regressions`, `jellyflow-open-gpui::testing::interaction_fixtures_cover_gpui_authoring_states` |
| Shader / Blueprint | `SampleGraphKind::ShaderGraph`, builtin `shader.blueprint`, GPUI `demo.shader.mix` schema probe | typed port rails, config controls, preview, invalid hover, typed commit rejection, blackboard action | `shader_graph_typed_ports_reject_incompatible_hover_and_commit`, `shader_sample_rejects_incompatible_typed_connections_through_default_store_path`, `authoring_interaction_states_have_regression_fixtures`, `jellyflow-open-gpui::testing::product_fixtures_cover_gpui_authoring_regressions`, `jellyflow-open-gpui::testing::authoring_interaction_report` |
| ERD / data model | `SampleGraphKind::Erd`, builtin `erd.table` | repeatable field rows, repeatable edit/remove/reorder, resize, slot bounds, handle-anchor proximity | `rich_node_resize_keeps_regions_and_handles_aligned`, `erd_snapshot_reports_semantic_region_measurements_to_runtime`, `erd_snapshot_places_table_handles_on_field_anchor_regions`, `jellyflow-open-gpui::testing::product_fixtures_cover_gpui_authoring_regressions`, `jellyflow-open-gpui::testing::authoring_interaction_report` |
| Mind map / knowledge canvas | `SampleGraphKind::MindMap`, builtin `mind-map.knowledge-canvas` | compact/full density, shell review, stable handles, graph-level visual coverage | `product_shape_snapshots_keep_authoring_regions_inside_nodes`, `density_modes_have_regression_coverage_for_rich_nodes`, gallery snapshot output, `jellyflow-open-gpui::testing::product_fixtures_cover_gpui_authoring_regressions` |

## Commands

Run these as the focused authoring regression gate:

```sh
cargo test -p jellyflow-egui --lib -- --nocapture
cargo nextest run -p jellyflow-open-gpui --no-fail-fast
cargo run -p jellyflow-egui --example gallery_snapshot -- target/jellyflow-egui-gallery
RUSTFLAGS='-Awarnings' cargo test --quiet --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --bin open-gpui-canvas-jellyflow
```

The broad plan gate still includes runtime, proof, template, examples, GPUI check, and format
commands from `docs/plans/2026-06-30-001-feat-node-ui-authoring-contracts-plan.md`.

## Adapter Rules

- Rich node snapshots must expand the adapter node rect to at least the renderer-reported
  `NodeRenderLayout::min_size` before reporting node measurements or handle bounds.
- `slot` remains the data lookup path. `anchor` is placement and port binding metadata. If a
  rendered region key differs from its descriptor anchor, adapters must report both names as
  aliases for measurement and handle layout.
- Shell density may hide rich controls, but visible handles and runtime anchor facts must remain
  queryable so wires do not drift.
- Resize preview geometry must use the same renderer-minimum-size path as committed snapshots.
- GPUI adapter-level gates now live in `jellyflow-open-gpui::testing`. The `canvas-jellyflow`
  example is a consumer/manual smoke surface, not the owner of product fixture regression logic.
- Reusable Open GPUI authoring glue belongs in `jellyflow-open-gpui`: JSON binding, live-store
  control planning, semantic repeatable action mapping, scoped element ids, and renderer host
  context routing. `canvas-jellyflow` should only own concrete component construction, local
  dispatch/focus/popup behavior, refresh notifications, and demo fixture defaults.
- `OpenGpuiProductFixtureReport` is the hard structured gate for fixture breadth. It must report
  `compact`, `regular`, and `full` density coverage, resize probes, region source evidence, action
  descriptors, inspector descriptors, blackboard descriptors, control primitives, repeatable rows,
  and layout-pass coverage facts.
- `OpenGpuiAuthoringInteractionReport` is the hard structured gate for interaction maturity. It
  must cover dropped-wire insert actions, node actions, measured/fallback/missing inspector targets,
  repeatable add/remove/reorder/edit with dynamic-port lifecycle evidence, blackboard actions,
  invalid shader hover rejection, and editable control regions.
- `OpenGpuiDynamicRepeatableLifecycleReport` is the hard structured gate for dynamic repeatable
  honesty. Adding a shader input must either create graph port facts or report
  `MissingGraphPort`; missing ports must not publish measured handles. Removing a bound shader
  input must remove its graph port and incident edges. Reordering keeps item, slot, anchor, port,
  and measured-row identity stable. ERD field edits refresh row data while staying downgraded when
  graph ports are missing. Dify-style parameter rows remain display-only and never publish handles.
- GPUI full measurement claims must be backed by Open GPUI `measured_element` layout-pass coverage.
  Projection fixture gates may prove clipping, controls, repeatables, menus, and inspector
  contracts, but they must keep capability reporting at `ProjectionFallback` or partial coverage
  unless every required region has live measured evidence.
- Custom renderer tests should prove the adapter resolves registered renderers, reports
  `MissingHostRenderer` and `UnregisteredRenderer` fallback reasons, and passes semantic slots,
  repeatables, action menus, measurement ids, and host services without importing Open GPUI widgets
  into runtime/core/layout.
- Shell is still a review state layered on top of density and capability reports. The current
  headless density enum is `compact` / `regular` / `full`; do not claim a separate productized shell
  mode until the semantic contract publishes one.
