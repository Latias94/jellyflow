---
type: "Verification Evidence"
title: "Open GPUI atomic node scene verification"
description: "Focused verification for the first atomic node scene implementation slice."
timestamp: 2026-07-05T12:10:58Z
tags: ["open-gpui", "canvas", "jellyflow", "verification", "nextest"]
status: "active"
related_plan: "docs/plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md"
git_branch: "feat/xyflow-product-surface"
verified_by: "cargo nextest run -p jellyflow-open-gpui --no-fail-fast; cargo test open-gpui-canvas scene/pointer filters; cargo check canvas-jellyflow"
---

# Verification

Focused verification was run for the first atomic node scene slice.
The goal was to prove the new data contracts and phase ordering without treating screenshots as the primary oracle.

- `cargo fmt --all --check`
- `cargo test -p jellyflow-open-gpui measured_region_kind_maps_to_interaction_role -- --nocapture`
- `cargo test -p jellyflow-open-gpui port_handle_plan_requires_visible_graph_bound_port -- --nocapture`
- `cargo test -p jellyflow-open-gpui node_surface_plan_preserves_renderer_context_facts_for_custom_and_fallback_paths -- --nocapture`
- `cargo nextest run -p jellyflow-open-gpui --no-fail-fast`
- `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -- --check`
- `cargo fmt --manifest-path repo-ref/open-gpui/crates/canvas/Cargo.toml -- --check`
- `git -C repo-ref/open-gpui diff --check`
- `cargo check --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow`
- `cargo test --manifest-path repo-ref/open-gpui/crates/canvas/Cargo.toml -p open-gpui-canvas pointer_owner -- --nocapture`
- `cargo test --manifest-path repo-ref/open-gpui/crates/canvas/Cargo.toml -p open-gpui-canvas scene_record_groups_keep_node_widgets_atomic_with_z_order -- --nocapture`
- `cargo test --manifest-path repo-ref/open-gpui/crates/canvas/Cargo.toml -p open-gpui-canvas scene_exposes_selected_node_chrome_and_tool_chrome_separately -- --nocapture`

# Result

All focused gates above passed.
Known Open GPUI warning noise remains out of scope: `gpui_macos` unexpected cfg warnings, the `block v0.1.6` future-incompat warning, and existing example dead-code/unused-import warnings from test helpers.

# Evidence

- Adapter tests prove measured region kinds map to explicit interaction roles and hidden/unbound anchors cannot satisfy connectable port-handle planning.
- Renderer tests prove custom and fallback paths keep the same surface plan facts.
- Canvas tests prove scene record groups preserve widget atomicity by z order and expose node-local/tool-chrome phases separately.
- Pointer-owner tests prove source handles win before node drag and empty pane remains selectable.
- The example build proves the scene-aware `canvas-jellyflow` host compiles against the local Open GPUI fork.

# Follow-up

- This verification does not yet replace native manual review for delayed first paint, overlap occlusion, and pointer feel.
- Continue with the plan's U4/U8 cleanup before declaring the full atomic scene plan done.

# Citations

- [Open GPUI Atomic Node Scene Plan](../../plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md)
- [Open GPUI canvas tests](../../../repo-ref/open-gpui/crates/canvas/src/gpui.rs)
- [Open GPUI pointer tests](../../../repo-ref/open-gpui/crates/canvas/src/tool.rs)
- [canvas-jellyflow example](../../../repo-ref/open-gpui/examples/canvas-jellyflow/src/main.rs)
