---
type: "Verification Evidence"
title: "Open GPUI interaction determinism gates"
description: "Verification Evidence for Open GPUI interaction determinism gates."
timestamp: 2026-07-04T19:02:03Z
tags: ["open-gpui", "jellyflow", "verification", "interaction"]
related_plan: "docs/plans/2026-07-05-001-refactor-open-gpui-interaction-determinism-plan.md"
git_branch: "feat/xyflow-product-surface"
verified_by: "cargo fmt; cargo nextest jellyflow-open-gpui; open-gpui canvas/example nextest"
---

# Verification

- `cargo fmt --all -- --check`
- `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -- --check`
- `cargo nextest run -p jellyflow-open-gpui --no-fail-fast`
- `cargo nextest run --manifest-path repo-ref/open-gpui/Cargo.toml -p open-gpui-canvas --lib --no-fail-fast`
- `cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow --no-fail-fast -E 'not test(product_gallery_screenshot_exporter_writes_nonblank_pngs_or_skips)'`
- `git diff --check`
- `git -C repo-ref/open-gpui diff --check`

# Result

All listed gates passed. The Open GPUI example run skipped only the known PNG
screenshot exporter by explicit test-name exclusion.

# Evidence

- `jellyflow-open-gpui`: 112 tests passed.
- `open-gpui-canvas`: 368 library tests passed.
- `open-gpui-canvas-jellyflow`: 81 tests passed, 1 known screenshot exporter
  test skipped.
- Focused product interaction gate passed:
  `tests::canvas_example_characterizes_current_product_interaction_gaps`.

# Follow-up

Existing Open GPUI platform warnings remain external noise from `gpui_macos`
`check-cfg` / `objc` macros and are not introduced by this change.

# Citations

- [Plan](../../plans/2026-07-05-001-refactor-open-gpui-interaction-determinism-plan.md)
- `crates/jellyflow-open-gpui/src/testing.rs`
- `repo-ref/open-gpui/examples/canvas-jellyflow/src/main.rs`
- `repo-ref/open-gpui/examples/canvas-jellyflow/src/visual_regression.rs`
