# Jellyflow Visible Elements Contract v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

JVE-010 and JVE-020 are complete: the lane is open, source coverage is recorded, and the runtime
visible node id contract exists.

Jellyflow now exposes the adapter-facing visible-node seam:

- `runtime::rendering::VisibleNodeIdsRequest` carries transform, logical viewport size, culling
  policy, node-origin fallback, and optional fallback size.
- `runtime::rendering::resolve_visible_node_ids` performs deterministic visible-node planning on
  top of `runtime::utils::get_nodes_inside`.
- `NodeGraphStore::visible_node_ids(viewport_size)` reads current view-state transform and resolved
  rendering interaction, including `only_render_visible_elements`.

The remaining seam is adapter-facing conformance/template coverage that can assert visible node ids
before renderer-specific smoke tests.

## Next Task

JVE-030: add conformance and template smoke coverage that lets adapters assert visible node ids
before renderer-specific smoke tests.

## Decisions Since Opening

- Scope v1 to visible node ids only.
- Keep visible edge ids as a follow-on because edge visibility needs endpoint/path/AABB policy.
- Keep real spatial indexing as a follow-on until adapter workload evidence proves the linear
  fallback is too slow.
- Use the current runtime linear scan as the behavior baseline so a future index can swap behind the
  same contract.
- Land the visible-node API in `runtime::rendering` beside existing renderer-neutral render-order
  helpers instead of creating a separate `runtime::visible` module.

## Validation To Run

JVE-020 passed:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime visible_node
cargo nextest run -p jellyflow-runtime --test public_surface
```

For JVE-030:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

For closeout:

```bash
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-visible-elements-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-visible-elements-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-visible-elements-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-visible-elements-contract-v1/CONTEXT.jsonl
git diff --check
```

## Next Recommended Action

Start JVE-030 by adding a conformance action/assertion for visible node ids, then wire the template
adapter smoke scenario to use the same contract.
