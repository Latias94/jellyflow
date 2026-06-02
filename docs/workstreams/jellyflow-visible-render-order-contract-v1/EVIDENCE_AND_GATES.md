# Jellyflow Visible Render Order Contract v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-02

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime visible_node_render_order
```

VRO-020 should add the first focused runtime repro.

## Gate Set

### Runtime Contract Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime visible_node_render_order
cargo nextest run -p jellyflow-runtime --test public_surface
```

### Conformance And Template Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

### Package And Closeout Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
```

### Metadata And Diff Gate

```bash
jq empty docs/workstreams/jellyflow-visible-render-order-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-visible-render-order-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-visible-render-order-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-visible-render-order-contract-v1/CONTEXT.jsonl
git diff --check
```

## Evidence Anchors

- `docs/workstreams/jellyflow-visible-render-order-contract-v1/DESIGN.md`
- `docs/workstreams/jellyflow-visible-render-order-contract-v1/TODO.md`
- `docs/workstreams/jellyflow-visible-render-order-contract-v1/TASKS.jsonl`
- `docs/workstreams/jellyflow-visible-render-order-contract-v1/CAMPAIGNS.jsonl`
- `docs/workstreams/jellyflow-visible-elements-contract-v1/DESIGN.md`
- `docs/workstreams/jellyflow-visible-elements-contract-v1/EVIDENCE_AND_GATES.md`
- `crates/jellyflow-runtime/src/runtime/rendering.rs`
- `crates/jellyflow-runtime/src/runtime/conformance/scenario/action.rs`
- `crates/jellyflow-runtime/src/runtime/conformance/runner/actions.rs`
- `templates/headless-adapter/src/lib.rs`

## Evidence Log

### 2026-06-02 - VRO-010 Workstream Opened

Scope: `docs/workstreams/jellyflow-visible-render-order-contract-v1`, `CONTEXT.md`

Result:

- Opened the visible render order contract lane from existing visible-node and render-order
  contracts.
- Set `VRO-020` as the first executable task.
- Kept visible edge culling, full scene render plans, renderer harnesses, and spatial indexing
  outside the first contract.

Behavior proven:

- Planning artifacts agree on target state, task order, gates, source coverage, and autonomous
  commit policy.

Fresh verification:

- Passed 2026-06-02:
  - `jq empty docs/workstreams/jellyflow-visible-render-order-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-visible-render-order-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-visible-render-order-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-visible-render-order-contract-v1/CONTEXT.jsonl`
  - `git diff --check`

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
