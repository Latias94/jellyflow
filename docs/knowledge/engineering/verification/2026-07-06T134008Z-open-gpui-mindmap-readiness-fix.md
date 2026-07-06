---
type: Verification Evidence
title: Open GPUI Mindmap readiness fix verification
timestamp: 2026-07-06T13:40:08Z
tags:
  - open-gpui
  - mindmap
  - verification
  - nextest
status: passed
git_branch: feat/xyflow-product-surface
source_workspace: /Users/frankorz/codes/rust/jellyflow
---

# Verified State

The Mindmap readiness fix was verified in both the Jellyflow workspace and the nested
`repo-ref/open-gpui` example workspace.

# Commands

- `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -- --check`
- `cargo fmt --all --check`
- `git diff --check`
- `git -C repo-ref/open-gpui diff --check`
- `RUSTFLAGS='-Awarnings' cargo test --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow mind_map_fixture_nodes_all_use_registered_product_renderers -- --nocapture`
- `RUSTFLAGS='-Awarnings' cargo test --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow product_renderer -- --nocapture`
- `RUSTFLAGS='-Awarnings' cargo test --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow mind_map_switch_reaches_readiness_without_pointer_events -- --nocapture`
- `RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-open-gpui product_fixture_catalog_lists_stable_gallery_cases --no-fail-fast --status-level fail --final-status-level fail`
- `RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-runtime builtin_product_kits_expose_readable_layout_budgets --no-fail-fast --status-level fail --final-status-level fail`
- `RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-runtime -p jellyflow-open-gpui --no-fail-fast --status-level fail --final-status-level fail`
- `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow --no-fail-fast --status-level fail --final-status-level fail -E 'not test(gallery_screenshot::product_gallery_screenshot_exporter_writes_nonblank_pngs_or_skips)'`
- `RUSTFLAGS='-Awarnings' cargo check --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow`
- `RUSTFLAGS='-Awarnings' cargo run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow`
- `HTTP_PROXY=http://127.0.0.1:10809 HTTPS_PROXY=http://127.0.0.1:10809 ALL_PROXY=http://127.0.0.1:10809 CARGO_HTTP_MULTIPLEXING=false RUSTFLAGS='-Awarnings' cargo check --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow`
- `HTTP_PROXY=http://127.0.0.1:10809 HTTPS_PROXY=http://127.0.0.1:10809 ALL_PROXY=http://127.0.0.1:10809 CARGO_HTTP_MULTIPLEXING=false RUSTFLAGS='-Awarnings' cargo test --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow mind_map -- --nocapture`

# Result

- Jellyflow targeted nextest gates passed.
- The Open GPUI canvas-jellyflow test suite passed with 89 tests passing and the known PNG exporter
  test skipped.
- Native launch reached the running state and was stopped manually with `Ctrl-C`.
- After pulling `repo-ref/open-gpui` from `origin/main`, targeted canvas-jellyflow check passed and
  the `mind_map` filtered tests passed: 2 passed, 0 failed, 88 filtered out.
- Existing `block v0.1.6` future-incompat warning remains unrelated to this fix.

# Citations

- `docs/knowledge/engineering/progress/2026-07-06T134008Z-open-gpui-mindmap-readiness-fix.md`
