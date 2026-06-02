# Jellyflow Visible Render Order Contract v1 - Evidence And Gates

Status: Closed
Last updated: 2026-06-02

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime visible_node_render_order
```

VRO-020 added this focused runtime repro.

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

### 2026-06-02 - VRO-020 Runtime Visible Render Order Contract

Scope: `crates/jellyflow-runtime/src/runtime`, `crates/jellyflow-runtime/src/runtime/tests`,
`crates/jellyflow-runtime/tests/public_surface.rs`

Result:

- Added `runtime::rendering::resolve_visible_node_render_order`.
- Added `NodeGraphStore::visible_node_render_order(viewport_size)`.
- Reused `resolve_visible_node_ids` and `resolve_node_render_order` so adapters receive one ordered
  visible-node list without duplicating runtime composition rules.
- Added focused rendering tests and public surface smoke.

Behavior proven:

- visible render order removes outside and hidden nodes while preserving draw order;
- selected visible nodes elevate after non-selected visible nodes;
- disabled culling matches the normal non-culling `node_render_order()` contract;
- public consumers can call the runtime helper and store helper.

Fresh verification:

- Passed 2026-06-02: `cargo fmt --check`
- Passed 2026-06-02: `cargo nextest run -p jellyflow-runtime visible_node_render_order`
- Passed 2026-06-02: `cargo nextest run -p jellyflow-runtime --test public_surface`

### 2026-06-02 - VRO-030 Conformance And Template Coverage

Scope: `crates/jellyflow-runtime/src/runtime/conformance`,
`crates/jellyflow-runtime/src/runtime/tests`, `templates/headless-adapter`

Result:

- Added `ConformanceAction::AssertVisibleNodeRenderOrder`.
- Added runner execution that compares `NodeGraphStore::visible_node_render_order(viewport_size)`
  with expected ordered ids.
- Added runtime conformance and adapter-conformance fixture tests.
- Added public surface serde coverage for the new fixture action.
- Added a visible-node-render-order scenario to the headless adapter template suite, increasing the
  built-in suite from 8 to 9 scenarios.

Behavior proven:

- conformance fixtures can assert ordered visible node ids without renderer traces;
- adapter conformance can run the same assertion through the fixture runner;
- the template adapter suite can save/check a suite containing both visible ids and ordered visible
  node render order assertions;
- the CLI `check` command reports the ordered visible-node scenario as an empty-trace assertion.

Fresh verification:

- Passed 2026-06-02: `cargo fmt --check`
- Passed 2026-06-02: `cargo nextest run -p jellyflow-runtime conformance`
- Passed 2026-06-02: `cargo nextest run -p jellyflow-runtime adapter_conformance`
- Passed 2026-06-02: `cargo test --manifest-path templates/headless-adapter/Cargo.toml`
- Passed 2026-06-02: `cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check`

### 2026-06-02 - VRO-040 Documentation And Closeout

Scope: `README.md`, `crates/jellyflow-runtime/README.md`, `CONTEXT.md`,
`docs/workstreams/jellyflow-visible-render-order-contract-v1`

Result:

- Documented ordered visible-node render runtime/store boundaries in root and runtime READMEs.
- Documented `ConformanceAction::assert_visible_node_render_order` as a pre-render adapter
  assertion.
- Updated `CONTEXT.md` to mark the visible-render-order contract workstream closed.
- Split visible edge culling, full scene render plans, renderer harnesses, and real spatial
  indexing into explicit follow-ons.
- Closed machine-readable workstream state.

Behavior proven:

- runtime package tests still pass with visible-node render order runtime and conformance coverage;
- clippy reports no runtime warnings under `-D warnings`;
- workstream metadata remains valid JSON and diff-clean.

Fresh verification:

- Passed 2026-06-02:
  - `cargo fmt --check`
  - `cargo nextest run -p jellyflow-runtime`
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
  - `jq empty docs/workstreams/jellyflow-visible-render-order-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-visible-render-order-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-visible-render-order-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-visible-render-order-contract-v1/CONTEXT.jsonl`
  - `git diff --check`

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
