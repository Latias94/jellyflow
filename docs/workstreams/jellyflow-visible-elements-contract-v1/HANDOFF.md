# Jellyflow Visible Elements Contract v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

JVE-010, JVE-020, and JVE-030 are complete: the lane is open, source coverage is recorded, the
runtime visible node id contract exists, and conformance/template smoke can assert it.

Jellyflow now exposes the adapter-facing visible-node seam:

- `runtime::rendering::VisibleNodeIdsRequest` carries transform, logical viewport size, culling
  policy, node-origin fallback, and optional fallback size.
- `runtime::rendering::resolve_visible_node_ids` performs deterministic visible-node planning on
  top of `runtime::utils::get_nodes_inside`.
- `NodeGraphStore::visible_node_ids(viewport_size)` reads current view-state transform and resolved
  rendering interaction, including `only_render_visible_elements`.

JVE-030 added:

- `ConformanceAction::AssertVisibleNodeIds`;
- runner comparison against `NodeGraphStore::visible_node_ids(viewport_size)`;
- runtime conformance and adapter-conformance fixture coverage;
- a headless adapter template visible-node scenario.

The remaining work is documentation, closeout evidence, and explicit follow-on split for visible
edge ids plus real spatial indexing.

## Next Task

JVE-040: document visible node runtime/adapter boundaries, record fresh evidence, and close or split
visible edge/spatial-index follow-ons.

## Decisions Since Opening

- Scope v1 to visible node ids only.
- Keep visible edge ids as a follow-on because edge visibility needs endpoint/path/AABB policy.
- Keep real spatial indexing as a follow-on until adapter workload evidence proves the linear
  fallback is too slow.
- Use the current runtime linear scan as the behavior baseline so a future index can swap behind the
  same contract.
- Land the visible-node API in `runtime::rendering` beside existing renderer-neutral render-order
  helpers instead of creating a separate `runtime::visible` module.
- Model visible-node conformance as an assertion action with no trace output, because the behavior
  is a renderer planning query rather than a mutating interaction.

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

These JVE-030 commands passed on 2026-06-02.

For closeout:

```bash
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-visible-elements-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-visible-elements-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-visible-elements-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-visible-elements-contract-v1/CONTEXT.jsonl
git diff --check
```

## Next Recommended Action

Start JVE-040 by updating README/runtime README/CONTEXT, then run the closeout package and metadata
gates. Keep visible edge ids and real spatial indexing as explicit follow-ons rather than broadening
this v1 contract.
