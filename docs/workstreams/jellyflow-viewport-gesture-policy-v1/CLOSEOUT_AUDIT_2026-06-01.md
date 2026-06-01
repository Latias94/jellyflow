# Jellyflow Viewport Gesture Policy v1 - Closeout Audit

Date: 2026-06-01
Status: Closed

## Scope Closed

This workstream added a renderer-neutral viewport gesture policy seam to `jellyflow-runtime`.

Shipped behavior:

- normalized scroll/pinch input resolves to `ViewportGestureIntent` or `ViewportGestureRejection`;
- normalized pointer drag-pan input resolves to intent or rejection;
- pan-on-scroll mode and speed are applied before store viewport mutation;
- zoom activation and Ctrl/pinch priority are tested;
- drag-pan respects configured buttons and rejects during connection or user selection;
- conformance fixture actions can execute accepted policy or assert expected rejection;
- public surface smoke covers the exported policy and fixture vocabulary.

## Files Changed

- `crates/jellyflow-runtime/src/runtime/viewport.rs`
- `crates/jellyflow-runtime/src/runtime/conformance/scenario/action.rs`
- `crates/jellyflow-runtime/src/runtime/conformance/runner/actions.rs`
- `crates/jellyflow-runtime/src/runtime/tests/viewport.rs`
- `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/fixture_runner.rs`
- `crates/jellyflow-runtime/tests/public_surface.rs`
- `docs/workstreams/jellyflow-viewport-gesture-policy-v1/`

## Gate Evidence

Passed on 2026-06-01:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime viewport
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime --test public_surface
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-viewport-gesture-policy-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-gesture-policy-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-gesture-policy-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-gesture-policy-v1/CONTEXT.jsonl
git diff --check
```

## ADR Compliance

- ADR-0001 satisfied: no renderer, Fret, wgpu, winit, or platform dependency was added.
- ADR-0003 satisfied: XyFlow feel was strengthened through headless runtime and adapter
  conformance tests rather than renderer tests inside core/runtime.

## Deferred Follow-ons

- Raw platform event normalization belongs in adapter crates.
- Pan inertia scheduling remains separate from this policy seam.
- Double-click zoom animation remains separate from this policy seam.
- Renderer smoke tests remain outside `jellyflow-core` and `jellyflow-runtime`.
