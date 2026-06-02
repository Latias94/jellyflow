# Jellyflow Node Resize Kernel v1 - Closeout Audit

Date: 2026-06-02

## Final Status

Closed.

## Completed Scope

- Opened the node resize kernel lane from XyFlow `XYResizer` evidence.
- Added renderer-neutral target-size resize planning under `runtime::resize`.
- Added min/max bounds, hidden/missing/no-op/invalid request rejection, and store apply support.
- Added XyFlow-style resize directions and node-origin-aware position planning.
- Added conformance action vocabulary and runner support through `apply_node_resize`.
- Added adapter conformance and template headless adapter resize coverage.
- Documented runtime/adapter resize boundaries in README files.

## Gates

All passed on 2026-06-02:

- `cargo fmt --check`
- `cargo nextest run -p jellyflow-runtime`
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
- `jq empty docs/workstreams/jellyflow-node-resize-kernel-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-resize-kernel-v1/TASKS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl`
- `git diff --check`

## Follow-Ons

- Exact pointer-resize session parity for XyFlow parent/child extent clamps and keep-aspect-ratio
  behavior, only after adapter evidence proves target-size resize planning is insufficient.
- Renderer smoke remains in future adapter crates, not in `jellyflow-core` or `jellyflow-runtime`.

## Residual Risk

The runtime now matches the stable target-size resize seam. It does not yet model raw pointer
start/delta lifecycle or XyFlow's full `getDimensionsAfterResize` clamp algorithm. That is
intentional until adapter evidence justifies a pointer-session request.
