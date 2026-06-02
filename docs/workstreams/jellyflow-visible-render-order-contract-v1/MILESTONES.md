# Jellyflow Visible Render Order Contract v1 - Milestones

Status: Closed
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

Exit criteria:

- Workstream artifacts exist and agree on scope, gates, current task, and non-goals.
- `CONTEXT.md` points at this active lane.
- JSON metadata validates.

Gate:

```bash
jq empty docs/workstreams/jellyflow-visible-render-order-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-visible-render-order-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-visible-render-order-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-visible-render-order-contract-v1/CONTEXT.jsonl
git diff --check
```

## M1 - Runtime Visible Render Order Contract

Exit criteria:

- Runtime exposes a pure helper for ordered visible node ids.
- Store exposes a helper that uses current viewport state, resolved rendering tuning, and selected
  node elevation.
- Focused tests cover enabled/disabled culling, hidden nodes, ordering, and elevation.

Gate:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime visible_node_render_order
cargo nextest run -p jellyflow-runtime --test public_surface
```

## M2 - Conformance And Template Coverage

Exit criteria:

- Conformance fixtures can assert ordered visible node ids without producing renderer traces.
- Adapter conformance tests cover the new assertion.
- Headless adapter template suite includes the assertion as a copyable example.

Gate:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

## M3 - Documentation And Closeout

Exit criteria:

- README/runtime docs describe the ordered visible node helper as an adapter pre-render contract.
- `CONTEXT.md`, `TODO.md`, `TASKS.jsonl`, `WORKSTREAM.json`, `EVIDENCE_AND_GATES.md`, and
  `HANDOFF.md` record final state.
- Follow-ons are explicitly split or deferred.

Gate:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-visible-render-order-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-visible-render-order-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-visible-render-order-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-visible-render-order-contract-v1/CONTEXT.jsonl
git diff --check
```
