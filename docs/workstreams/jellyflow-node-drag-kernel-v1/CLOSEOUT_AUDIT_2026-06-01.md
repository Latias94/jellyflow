# Jellyflow Node Drag Kernel v1 - Closeout Audit

Date: 2026-06-01

## Final Status

Closed. JND-010 through JND-060 are complete.

## Completed Outcomes

- Opened a renderer-free node drag kernel lane from the interaction harness follow-on.
- Added public `runtime::drag` planning and apply helpers around normal `NodeGraphStore`
  transactions.
- Built deterministic drag items from a primary node plus current selected nodes.
- Applied draggable policy, hidden-node filtering, and selected group parent-child filtering.
- Added shared snap-to-grid offsets for multi-selection drag.
- Added global extent group clamping, per-node rect extent clamping, and node-origin-aware bounds
  math.
- Added basic `NodeExtent::Parent` resolution to parent group rects when `expand_parent` is false.
- Added renderer-neutral node drag gesture payloads for start/update/end with canvas-space pointer
  intent.
- Wired XyFlow-compatible node drag callbacks and `NodeChange::Position` projection coverage.
- Updated README material with the headless node drag strategy.

## Review

`review-workstream` self-review found no blocking findings.

- Workstream compliance: all task ledger items are complete, target state is met, and renderer
  dependencies stayed out of scope.
- Code quality: drag behavior remains behind a small renderer-neutral planning API, normal graph
  transactions, and existing store/callback infrastructure.
- Missing gates: none after closeout verification.
- Residual risk: adapter-owned pointer capture remains out of scope; parent expansion, auto-pan,
  public fixture format, and renderer smoke tests are follow-ons.

## Verification

`verify-rust-workstream` closeout claim: the node drag kernel lane is documented and complete, and
the runtime package remains formatted, tested, lint-clean, JSON-valid, and diff-clean.

- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime`: passed with 150 tests.
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
- `jq empty docs/workstreams/jellyflow-node-drag-kernel-v1/WORKSTREAM.json`: passed.
- `git diff --check`: passed.

## Follow-Ons

- Parent expansion behavior for group resizing during drag.
- Auto-pan as a separate headless viewport-plus-drag contract.
- Public fixture format after more gesture families settle.
- Renderer adapter smoke tests in future wgpu, egui, Fret, or other adapter crates.
