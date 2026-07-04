---
type: "Work Progress"
title: "Open GPUI Product Polish Layout-Pass Evidence U1-U8"
description: "Execution summary for the Open GPUI product polish and layout-pass evidence plan."
tags: ["open-gpui", "product-polish", "layout-pass", "ce-work"]
timestamp: 2026-07-04T16:10:00+08:00
status: "active"
related_plan: "docs/plans/2026-07-03-001-feat-open-gpui-product-polish-layout-pass-plan.md"
git_branch: "feat/xyflow-product-surface"
---

# Summary

`docs/plans/2026-07-03-001-feat-open-gpui-product-polish-layout-pass-plan.md`
has been executed through U7 and is in U8 documentation closeout.

The mature boundary remains Open GPUI-first:

- runtime owns headless semantic descriptors and invalidation intent;
- `jellyflow-open-gpui` owns widget-free adapter ids, authoring plans,
  measurement ids, coverage reports, product fixture reports, graph affordance
  evidence, and hard gates;
- `repo-ref/open-gpui/examples/canvas-jellyflow` owns concrete Open GPUI
  widgets, host-local component atoms, measured-element collection,
  focus/pointer arbitration, screenshots, native lifecycle smoke, and product
  visual layout.

egui and Dioxus remain semantic-contract targets rather than mature adapter
peers for this stage. There is still no shared cross-framework widget crate.

# Verified State

- Root commit `b9df327` requires explicit drag-exclusion measured internals
  evidence in `jellyflow-open-gpui::testing`.
- Local `repo-ref/open-gpui` commit `4e84d7e` stops `canvas-jellyflow`
  projection visual reports from inferring drag-exclusion regions from control
  counts, projected controls, or renderer presets.
- Focused checks passed:
  - `cargo fmt --all`
  - `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml`
  - `RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-open-gpui measured_internals_evidence_requires_explicit_drag_exclusion_regions --no-fail-fast`
  - `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow projection_visual_report_does_not_infer_drag_exclusion_from_controls canvas_example_collects_host_product_surface_report --no-fail-fast`
  - `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow product_gallery_native_smoke_covers_launch_drag_and_close product_gallery_fixtures_project_non_overlapping_node_bounds product_port_hotspot_path_resolves_measured_handle_endpoint product_reconnect_sequence_report_covers_endpoint_switches_and_recovery dropped_wire_gesture_commits_insert_from_connect_release projection_visual_report_does_not_infer_drag_exclusion_from_controls --no-fail-fast`

# Next Action

Finish U8 docs/memory validation, commit the documentation closeout, then run
the final ce-work shipping checks before marking the active goal complete.

# Citations

- [Plan](../../../plans/2026-07-03-001-feat-open-gpui-product-polish-layout-pass-plan.md)
- [README](../../../crates/jellyflow-open-gpui/README.md)
- [Current state](../current-state.md)
- [Memory log](../log.md)
