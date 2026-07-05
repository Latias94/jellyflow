---
type: "Verification Evidence"
title: "Open GPUI scene-host cleanup verification"
description: "Verification for the canvas-jellyflow scene-host product path cleanup."
timestamp: 2026-07-05T12:37:26Z
tags: ["open-gpui", "canvas-jellyflow", "verification"]
status: "active"
related_plan: "docs/plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md"
git_branch: "feat/xyflow-product-surface"
verified_by: "cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -- --check; cargo check --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow; cargo test scene host filters"
---

# Verification

- `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -- --check`
- `cargo check --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow`
- `cargo test --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow node_scene -- --nocapture`
- `cargo test --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow canvas_view_scene_node_host_prefers_scene_frame_before_bootstrap -- --nocapture`
- `cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow --no-fail-fast`

# Result

All commands passed.
The full example nextest gate ran 88 tests and passed; the PNG exporter test was slow at about 135 seconds but did not fail.
The first attempted parallel test command used two Cargo test filters and failed because Cargo accepts one test filter; it was rerun with the shared `node_scene` filter.
Known Open GPUI warning noise remains unchanged.

# Evidence

- `initial_node_scene_bootstrap_records_follow_canvas_z_order` proves the document-only path is explicitly named as bootstrap fallback.
- `node_scene_widgets_render_from_scene_record_groups` proves scene record groups still drive node widget order.
- `canvas_view_scene_node_host_prefers_scene_frame_before_bootstrap` proves the host chooses prepared scene records before last-bounds recomputation, and only then falls back to initial document bootstrap.

# Follow-up

- Native manual launch review still remains for the full plan DoD.
- The next implementation slice should focus on renderer layout polish and deleting remaining non-measured fit assumptions.

# Citations

- [Open GPUI Atomic Node Scene Plan](../../plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md)
- [canvas-jellyflow main](../../../repo-ref/open-gpui/examples/canvas-jellyflow/src/main.rs)
