# Node UI Authoring Regression Gates

Jellyflow's rich-node regression gates are split between headless contract tests, adapter geometry
tests, and local visual review. Pixel snapshots are useful for review, but the hard gate is geometry
and semantic capability evidence that does not depend on GPU output.

## Product Shape Matrix

| Shape | Fixture | Required states | Hard evidence |
| --- | --- | --- | --- |
| Dify-style workflow | `SampleGraphKind::AutomationBuilder`, builtin `workflow.automation`, GPUI `workflow.review` / `demo.llm` | full, compact, shell review, resize, dropped-wire menu, inspector/action descriptors, native prompt/model/temperature controls | `product_shape_snapshots_keep_authoring_regions_inside_nodes`, `density_modes_have_regression_coverage_for_rich_nodes`, `dropped_wire_menu_is_backed_by_authoring_action_descriptors`, `authoring_interaction_states_have_regression_fixtures`, `jellyflow-open-gpui::testing::product_fixtures_cover_gpui_authoring_regressions`, `jellyflow-open-gpui::testing::interaction_fixtures_cover_gpui_authoring_states`, `jellyflow-open-gpui::testing::assert_product_interaction_report_gates`, `canvas_example_collects_host_product_surface_report` |
| Shader / Blueprint | `SampleGraphKind::ShaderGraph`, builtin `shader.blueprint`, GPUI `shader.material_mix` / `demo.shader.mix` | typed port rails, config controls, dynamic input repeatables, missing-port diagnostics, invalid hover, typed commit rejection, blackboard action | `shader_graph_typed_ports_reject_incompatible_hover_and_commit`, `shader_sample_rejects_incompatible_typed_connections_through_default_store_path`, `authoring_interaction_states_have_regression_fixtures`, `jellyflow-open-gpui::testing::product_fixtures_cover_gpui_authoring_regressions`, `jellyflow-open-gpui::testing::authoring_interaction_report`, `jellyflow-open-gpui::testing::dynamic_repeatable_lifecycle_report`, `jellyflow-open-gpui::testing::assert_product_interaction_report_gates` |
| ERD / data model | `SampleGraphKind::Erd`, builtin `erd.table`, GPUI `erd.customer_orders` | repeatable field rows, repeatable edit/remove/reorder, resize, slot bounds, handle-anchor proximity, explicit missing-port downgrade | `rich_node_resize_keeps_regions_and_handles_aligned`, `erd_snapshot_reports_semantic_region_measurements_to_runtime`, `erd_snapshot_places_table_handles_on_field_anchor_regions`, `jellyflow-open-gpui::testing::product_fixtures_cover_gpui_authoring_regressions`, `jellyflow-open-gpui::testing::authoring_interaction_report`, `jellyflow-open-gpui::testing::dynamic_repeatable_lifecycle_report`, `jellyflow-open-gpui::testing::assert_product_interaction_report_gates` |
| Mind map / knowledge canvas | `SampleGraphKind::MindMap`, builtin `mind-map.knowledge-canvas`, GPUI `mind-map.strategy` | compact/full density, shell review, stable handles, graph-level visual coverage, topic/source custom renderers | `product_shape_snapshots_keep_authoring_regions_inside_nodes`, `density_modes_have_regression_coverage_for_rich_nodes`, gallery snapshot output, `jellyflow-open-gpui::testing::product_fixtures_cover_gpui_authoring_regressions`, `jellyflow-open-gpui::testing::assert_product_interaction_report_gates`, `jellyflow-open-gpui::testing::assert_screenshot_region_report_gates`, `canvas-jellyflow::product_gallery_screenshot_exporter_writes_nonblank_pngs_or_skips` |

## Commands

Run these as the focused authoring regression gate:

```sh
cargo test -p jellyflow-egui --lib -- --nocapture
cargo nextest run -p jellyflow-open-gpui --no-fail-fast
cargo run -p jellyflow-egui --example gallery_snapshot -- target/jellyflow-egui-gallery
cargo test --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --features open_gpui_platform/runtime_shaders --bin open-gpui-canvas-jellyflow -- --nocapture --test-threads=1
```

The broad plan gate still includes runtime, proof, template, examples, GPUI check, and format
commands from `docs/plans/2026-06-30-001-feat-node-ui-authoring-contracts-plan.md`.
Open GPUI screenshot smoke artifacts, when the local headless renderer supports capture, are written
under `repo-ref/open-gpui/target/open-gpui-jellyflow-gallery/`. The exporter now pairs those PNGs
with coarse product-region evidence for node bodies, node-internal UI, wire paths, and port areas.

For the Open GPUI canvas/node-UI foundation gate, run the broader set:

```sh
cargo fmt --all -- --check
cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -- --check
git diff --check
git -C repo-ref/open-gpui diff --check
cargo nextest run -p jellyflow-open-gpui --no-fail-fast
cargo nextest run -p jellyflow-runtime -p jellyflow-egui -p jellyflow-proof --lib --no-fail-fast
cargo test -p jellyflow-runtime --test public_surface -- --nocapture
cargo test --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --features open_gpui_platform/runtime_shaders --bin open-gpui-canvas-jellyflow -- --nocapture --test-threads=1
```

Native lifecycle smoke now has a structured Open GPUI test gate:

```sh
cargo test --manifest-path repo-ref/open-gpui/crates/gpui/Cargo.toml test_simulate_window_close_honors_last_window_quit_mode -- --nocapture
cargo test --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --features open_gpui_platform/runtime_shaders product_gallery_native_smoke -- --nocapture --test-threads=1
```

This gate uses `TestAppContext::simulate_window_close` to exercise the App-side window removal path
that `QuitMode::LastWindowClosed` observes. The `canvas-jellyflow` smoke opens the real
`JellyflowCanvasView`, verifies the product gallery is rendered, drags a product node without
resetting sibling node positions, closes the last test window, and asserts that the test platform
received an app quit request. A blank window report must fail the native smoke gate.

Screenshot smoke now has a structured region-evidence gate:

```sh
cargo test -p jellyflow-open-gpui screenshot_region -- --nocapture
cargo test --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --features open_gpui_platform/runtime_shaders screenshot -- --nocapture --test-threads=1
```

`OpenGpuiScreenshotRegionReport` distinguishes skipped capture, blank/single-color captures,
missing product fixtures, missing ROI categories, and blank ROI categories. Screenshot files remain
review aids; the hard claim is the serializable adapter evidence, not pixel-golden image equality.

Native review smoke remains a manual macOS launch check for the real windowing shell:

```sh
cargo run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --features open_gpui_platform/runtime_shaders --bin open-gpui-canvas-jellyflow
```

For that smoke, verify the product gallery launches, Dify/shader/ERD/mind-map nodes render with
internal UI, nodes drag from headers/passive regions, wires and ports are visible, reconnect handles
are reachable on selected edges, and closing the last macOS window exits. The structured test gate
proves the Open GPUI close/quit contract; this manual launch is still useful for OS-shell behavior
and visual review.

## Open GPUI Product Interaction Gates

The mature Open GPUI path uses structured reports as hard gates. Screenshot output and native
manual review are aids for diagnosing what a failure looks like; they are not the source of truth.

- `assert_product_fixture_regression_gates` protects adapter-level product breadth: Dify-style
  workflow, shader/blueprint, ERD, and mind-map fixtures must expose semantic regions, density,
  resize, action/menu/inspector/blackboard, dynamic-port, and layout-pass coverage facts.
- `assert_authoring_interaction_regression_gates` protects headless authoring semantics:
  dropped-wire actions, node actions, inspector target sources, blackboard actions, invalid shader
  hover rejection, editable controls, and repeatable add/remove/reorder/edit lifecycle evidence.
- `assert_product_gallery_host_report_gates` protects host renderer coverage: each product fixture
  must be served by its expected Open GPUI product renderer instead of descriptor fallback.
- `assert_native_lifecycle_evidence_gates` protects the native smoke claim: the real product gallery
  must render, a product node drag must be checked, and the closest available Open GPUI last-window
  close path must observe app quit. A blank window or skipped close automation is not accepted as a
  hard pass.
- `assert_host_visual_interaction_report_gates` protects visual/layout facts: node-internal content
  must remain visible when unselected, satisfy readable budgets, stay inside node bounds, avoid
  handle overlap, use fresh measured regions, keep repeatable anchors, avoid initial node overlap,
  bound invalid-hover and dropped-wire UI, keep edge endpoints tied to measured handles, and include
  `OpenGpuiMeasuredInternalsEvidence`. Measured internals evidence records node-bound source,
  handle coverage, readable-region coverage, drag-exclusion coverage, stale regions, and explicit
  component-declared overflow. It does not estimate text/control fit from line counts, character
  widths, or preset row heights.
- `assert_product_interaction_report_gates` protects concrete product interactions: product cards
  need full drag pointer sequences, controls must be event-shielded, ports must be reachable through
  the host hotspot path, Select/Pan/Connect tooling must be visible, connect/reconnect/dropped-wire
  gestures must synchronize through Jellyflow store transactions, and reconnect evidence must cover
  source endpoint switching, target endpoint switching, invalid rollback, empty drops, and a second
  gesture after rejection. Hidden repeatable overflow must have a visible indicator. The gate also
  requires `OpenGpuiGraphAffordanceEvidence`: committed wires and previews must share product route
  policy, direct-line preview fallback is rejected, port and reconnect hit budgets must be large
  enough, and drag/readable layout regions must be reported.
- `assert_screenshot_region_report_gates` protects screenshot usefulness without turning PNGs into
  pixel goldens: every product fixture capture must be nonblank and must include present node-body,
  node-internal UI, wire-path, and port-area ROI evidence.
- `product_gallery_screenshot_exporter_writes_nonblank_pngs_or_skips` is a smoke/review aid. It may
  skip when Open GPUI has no headless renderer; when it writes files, every product fixture must
  produce a nonblank PNG and a passing `OpenGpuiScreenshotRegionReport`.

## Canvas Node UI Foundation Gates

These gates cover the product feel gap that screenshot review previously caught too late.

- Drag feel is protected by full pointer sequence evidence plus the real
  `canvas_view_keeps_drag_state_while_syncing_product_surface_moves` path: syncing node transforms
  into `NodeGraphStore` must not rebuild the editor in a way that drops active drag state.
- Wire style is protected at two levels. Generic Open GPUI canvas tests cover route geometry,
  previews, hit testing, and reconnect release events; `jellyflow-open-gpui` reports whether the
  product host uses route policy evidence instead of direct-line preview fallback.
- Port and reconnect UX is protected by visible hotspot/reconnect evidence. The host must report
  reachable port hotspots, selected-edge reconnect affordances, store-synced reconnect outcomes, and
  invalid-target rollback without repeated planning errors.
- Node-internal UI readability is protected by runtime readable-size hints, host-local component
  layout tests, and measured internals evidence. Product renderers may downgrade or clamp their own
  UI and may report explicit overflow, but adapter gates must not infer arbitrary text/control fit
  from preset thresholds.
- Screenshot smoke is intentionally weaker than geometry and interaction gates. It proves review
  artifacts and coarse product regions can be generated for product families when supported, but it
  is not a pixel-golden oracle.

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
- Open GPUI product renderers should be registered through `OpenGpuiNodeRendererRegistry` and
  consumed via host-local renderer closures. `canvas-jellyflow` currently maps `decision-card`,
  `shader-card`, `table-card`, `topic-card`, and `source-card` into native GPUI component trees.
- The Open GPUI node component kit remains host-local. It can provide `TextInput`, `Textarea`,
  `NumberInput`, `Select`, `Switch`, `Slider`, menu/action buttons, repeatable rows, event shielding,
  and `measured_element` wrappers, but it must not move GPUI element types into runtime or
  `jellyflow-open-gpui`.
- Advanced controls are honest partial/stub states in this stage: code editor and color render as
  partial badges; asset picker, variable picker, and port-binding picker render as disabled
  stub/display controls. Tests should assert capability facts instead of pretending these are
  complete product widgets.
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
- Screenshot export is a smoke/review aid, not a golden oracle. A platform without a headless
  renderer may skip the screenshot test, but structured host reports must still pass.
- Open GPUI product gallery layout must apply adapter-local readable minimum sizes before using
  full-density renderers. Silent clipping is a failed state: `OpenGpuiHostVisualInteractionReport`
  rows must satisfy both `content_readable` and `content_within_node_bounds`.
- Open GPUI product renderers should derive region placement from host-local adaptive layout plans,
  not fixed absolute row constants. Full/compact/shell degradation is a renderer policy; runtime
  supplies semantic density and overflow intent, and `jellyflow-open-gpui` reports the resulting
  evidence without owning widgets.
- Product fixture reports must include pairwise node-bounds overlap evidence. Dify, shader, ERD,
  and mind-map fixtures should be authored against the adapter's readable size budgets so the
  default launch is a valid review state, not a post-layout recovery case.
- `canvas-jellyflow` must use actual `Bounds<Pixels>` from the Open GPUI canvas paint path for
  input mapping and viewport sizing. Initial load and fixture switches may auto-fit once; later
  resize after user interaction should preserve the current document center.
- Node-internal event shielding is limited to real interactive controls. Product card headers,
  shells, and passive display regions are drag surfaces; buttons, text inputs, menus, sliders, and
  repeatable action controls may stop propagation locally.
- Runtime builtin fixture coordinates may encode semantic example topology, but concrete widget
  row heights, density fallbacks, and component event policy remain Open GPUI host/example
  responsibilities. Adapter-level evidence is limited to graph-owned hitboxes, measured internals,
  and explicit host reports.
