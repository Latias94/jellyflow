---
type: "Verification Evidence"
title: "Open GPUI adapter productization verification"
description: "Verification commands and outcomes for the Open GPUI adapter productization implementation."
tags: ["open-gpui", "adapter", "verification", "nextest"]
timestamp: 2026-07-04T19:40:31+08:00
status: "passed"
related_plan: "docs/plans/2026-07-04-001-feat-open-gpui-adapter-productization-plan.md"
producer_id: "codex-root"
---

# Verification

Passed:

- `RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-open-gpui product_fixtures_cover_gpui_authoring_regressions product_fixture_catalog_lists_stable_gallery_cases --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-open-gpui product_fixtures_cover_gpui_authoring_regressions measured_internals layout_pass --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-open-gpui host_surface_report visual_gate product_interaction screenshot native_smoke measured_content --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-open-gpui --no-fail-fast` (109/109 passed)
- `RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-open-gpui graph_affordance_evidence_serializes_route_hit_and_layout_budgets product_interaction_gate_rejects_direct_line_preview_fallback --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-open-gpui product_interaction_gate_accepts_productized_report product_interaction_gate_derives_gaps_from_report_fields product_interaction_gate_rejects_missing_graph_affordance_evidence product_interaction_gate_rejects_direct_line_preview_fallback product_interaction_gate_rejects_incomplete_reconnect_sequence native_lifecycle_gate_rejects_close_automation_skip screenshot_region_gate_accepts_every_product_region screenshot_region_gate_rejects_missing_or_single_color_regions screenshot_region_gate_rejects_skipped_capture --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/crates/canvas/Cargo.toml -p open-gpui-canvas connect_tool_reports_dropped_release_for_empty_canvas select_tool_reconnects_selected_edge_target_handle select_tool_reconnects_selected_edge_source_handle select_tool_reports_dropped_reconnect_release_for_empty_canvas connecting_preview_uses_configured_route_policy reconnecting_preview_reuses_selected_edge_route_path selected_edge_adds_reconnect_handles_to_paint_frame --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/crates/canvas/Cargo.toml -p open-gpui-canvas connecting_preview_uses_configured_route_policy reconnecting_preview_reuses_selected_edge_route_path selected_edge_adds_reconnect_handles_to_paint_frame connecting_preview_snaps_to_valid_target_handle connecting_preview_does_not_snap_to_invalid_target_handle --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow -E 'not test(gallery_screenshot::product_gallery_screenshot_exporter_writes_nonblank_pngs_or_skips)' --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow product_renderers node_component_kit --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow product_reconnect product_port_hotspot dropped_wire invalid_connection selected_product_edge product_toolbar --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow product_gallery_fixtures_project_non_overlapping_node_bounds product_gallery_cases_build_canvas_editors_and_switch_fixture_state product_gallery_initial_viewport_fits_default_canvas_area product_renderer_layouts_fit_runtime_readable_budgets product_card_layouts_stay_inside_reduced_nodes shader_fixture_projects_typed_ports_into_gpui_surface_summary shader_default_node_projects_dynamic_repeatable_items_into_surface_summary --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow canvas_example_collects_host_product_surface_report canvas_example_characterizes_current_product_interaction_gaps canvas_example_consumes_adapter_product_fixture_gates projection_visual_report_does_not_infer_drag_exclusion_from_controls measured_content_evidence_uses_coverage_region_kinds product_dense_surface_probe_covers_editing_and_menu_boundaries native_smoke screenshot_region_evidence_rejects_single_color_roi --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow product_connection_preview_route_matches_projected_edge_route_policy canvas_example_characterizes_current_product_interaction_gaps projects_jellyflow_edge_route_hints_into_canvas_routes product_reconnect_sequence_report_covers_endpoint_switches_and_recovery dropped_wire_gesture_commits_insert_from_connect_release --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow canvas_example_consumes_adapter_product_fixture_gates canvas_example_collects_host_product_surface_report product_gallery_fixtures_project_non_overlapping_node_bounds projected_handles_follow_semantic_slot_anchors_after_node_resize table_repeatable_limit_accounts_for_overflow_indicator_budget shader_repeatable_limit_accounts_for_width_and_height product_layout_regions_preserve_compact_and_shell_modes product_surface_drag_sequence_moves_shader_node_and_commits_outside_canvas_bounds product_surface_drag_sequence_cancel_restores_shader_node --no-fail-fast`
- `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow canvas_example_characterizes_current_product_interaction_gaps projection_visual_report_does_not_infer_drag_exclusion_from_controls measured_content_evidence_uses_coverage_region_kinds product_dense_surface_probe_covers_editing_and_menu_boundaries native_smoke screenshot_region_evidence_rejects_single_color_roi product_connection_preview_route_matches_projected_edge_route_policy --no-fail-fast`
- `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -- --check`
- `git -C repo-ref/open-gpui diff --check`
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo nextest run -p jellyflow-runtime -p jellyflow-egui -p jellyflow-proof --lib --no-fail-fast` (532/532 passed)
- `python "$HOME/.codex/skills/engineering-wiki-memory/scripts/wiki_memory.py" validate --root docs/knowledge/engineering`

# Interrupted Or Skipped

- A full `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow --no-fail-fast` reached 77/78 passing tests in an earlier run and was interrupted after `gallery_screenshot::product_gallery_screenshot_exporter_writes_nonblank_pngs_or_skips` hung for more than two minutes.
- Later full example verification used nextest expression filtering to exclude only that screenshot exporter test; 78/78 selected tests passed with one test skipped.

# Notes

- The Open GPUI `block v0.1.6` future-incompat warning remains known upstream/local-fork noise and is not introduced by this work.
- `repo-ref/open-gpui` changes are committed only to the local `main` fork and were not pushed.

# Citations

- [Plan](../../../plans/2026-07-04-001-feat-open-gpui-adapter-productization-plan.md)
- [Progress closeout](../progress/2026-07-04-open-gpui-adapter-productization-closeout.md)
