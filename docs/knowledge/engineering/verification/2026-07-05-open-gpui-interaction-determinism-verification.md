---
type: "Verification Evidence"
title: "Open GPUI interaction determinism verification"
tags: ["open-gpui", "jellyflow-open-gpui", "nextest", "verification"]
timestamp: 2026-07-05T01:52:32+08:00
status: "passed"
related_plan: "../../plans/2026-07-05-001-refactor-open-gpui-interaction-determinism-plan.md"
git_branch: "feat/xyflow-product-surface"
---

# Verified State

The current slice passed the generic canvas gesture gate, the full selected `canvas-jellyflow` product interaction regression gate, and the widget-free Open GPUI adapter report crate gate.

# Commands

- `git diff --check`
- `git -C repo-ref/open-gpui diff --check`
- `cargo fmt --all -- --check`
- `cargo fmt --manifest-path repo-ref/open-gpui/Cargo.toml --all -- --check`
- `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --all -- --check`
- `cargo nextest run --manifest-path repo-ref/open-gpui/Cargo.toml -p open-gpui-canvas --lib --no-fail-fast`
- `cargo nextest run -p jellyflow-open-gpui --no-fail-fast`
- `cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow --no-fail-fast -E 'not test(product_gallery_screenshot_exporter_writes_nonblank_pngs_or_skips)'`

# Results

- `open-gpui-canvas` lib tests: 368/368 passed.
- `jellyflow-open-gpui`: 109/109 passed.
- `canvas-jellyflow` example tests: 81/81 passed, with the known PNG exporter test excluded.
- Existing Open GPUI macOS `unexpected cfg cargo-clippy` warnings and example dead-code warnings remain out of scope for this slice.

# Citations

- [Plan](../../plans/2026-07-05-001-refactor-open-gpui-interaction-determinism-plan.md)
- [Progress note](../progress/2026-07-05-open-gpui-interaction-determinism-progress.md)
