# Jellyflow Node Resize Kernel v1 - Milestones

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

Exit criteria:

- Problem and target state are explicit.
- XyFlow `xyresizer` source coverage is linked.
- Non-goals keep DOM, renderer, resize handles, screenshots, and pixels outside this lane.
- First executable task is `JNR-020`.

Status: complete on 2026-06-02.

## M1 - Minimal Pure Resize Planner

Exit criteria:

- Runtime exposes or internally owns a renderer-neutral resize request/plan shape.
- A single-node resize can produce deterministic `SetNodeSize` transactions.
- Min/max size bounds reject or clamp invalid targets deterministically.
- Focused runtime tests prove accepted, rejected, and no-op behavior.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime resize
cargo nextest run -p jellyflow-runtime --test public_surface
```

Status: complete on 2026-06-02. `runtime::resize` now plans single-node `SetNodeSize`
transactions, clamps optional min/max bounds, rejects hidden/missing/no-op/invalid requests, and is
covered through focused runtime tests plus the public surface smoke.

## M2 - Origin, Position, And Extent Constraints

Exit criteria:

- Left/top controls can plan position and size changes when in scope.
- Node origin and parent extents are handled or split with evidence.
- Child extents and keep-aspect-ratio behavior are implemented or split with evidence.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime resize
cargo nextest run -p jellyflow-runtime drag_parent_expansion
```

Status: pending.

## M3 - Conformance And Template Integration

Exit criteria:

- Resize traces can be represented through existing or new conformance actions.
- Adapter conformance covers accepted resize transactions and callback counts.
- Template smoke includes resize only after the runtime planner interface is stable.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

Status: pending.

## M4 - Documentation And Closeout

Exit criteria:

- README/runtime README explain what runtime owns versus adapter resize-handle ownership.
- Workstream evidence is current and machine-readable state is valid.
- Remaining exact XyFlow parity, renderer smoke, or platform follow-ons are split or deferred.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-node-resize-kernel-v1/WORKSTREAM.json docs/workstreams/jellyflow-node-resize-kernel-v1/TASKS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl
git diff --check
```

Status: pending.
