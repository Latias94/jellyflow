---
type: "Verification Evidence"
title: "Open GPUI atomic node scene closeout verification"
timestamp: 2026-07-05T21:44:49+08:00
status: passed
git_branch: feat/xyflow-product-surface
related_plan: docs/plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md
tags: ["open-gpui", "atomic-scene", "verification"]
---

# Gates

- Passed: `cargo fmt --manifest-path repo-ref/open-gpui/crates/canvas/Cargo.toml -- --check`
- Passed: `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -- --check`
- Passed: `cargo fmt --all --check`
- Passed: `git diff --check && git -C repo-ref/open-gpui diff --check`
- Passed: `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/crates/canvas/Cargo.toml -p open-gpui-canvas --lib --no-fail-fast --status-level fail --final-status-level fail`
- Passed: `RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-runtime -p jellyflow-open-gpui --no-fail-fast --status-level fail --final-status-level fail`
- Passed with documented exclusion: `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow --no-fail-fast --status-level fail --final-status-level fail -E 'not test(gallery_screenshot::product_gallery_screenshot_exporter_writes_nonblank_pngs_or_skips)'`
- Passed: `RUSTFLAGS='-Awarnings' cargo check --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow`
- Passed: native launch smoke with `RUSTFLAGS='-Awarnings' cargo run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow`; the app reached running state and was stopped with SIGINT.

# Test Counts

- `open-gpui-canvas` lib: 374 tests passed.
- `jellyflow-runtime` and `jellyflow-open-gpui`: 587 tests passed across 3 binaries.
- `open-gpui-canvas-jellyflow`: 87 tests passed with only the known PNG exporter smoke test skipped.

# Notes

The interrupted full example run reached 87 passed tests before the PNG exporter smoke test exceeded 180 seconds. This matches the plan's allowed PNG-exporter-only exclusion path for the known exporter hang and does not indicate a product renderer or scene ordering failure.

# Citations

- [Atomic node scene plan](../../plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md)
- [Closeout progress](../progress/2026-07-05T134449Z-open-gpui-atomic-node-scene-closeout.md)
