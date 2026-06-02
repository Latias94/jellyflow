# Jellyflow Viewport Pan Inertia Scheduling v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-02

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime pan_inertia
```

This lane starts with a missing test filter; JPIS-020 should add focused pan inertia tests.

## Gate Set

### Pure Planner Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime pan_inertia
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
jq empty docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CONTEXT.jsonl
git diff --check
```

## Evidence Anchors

- `docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/DESIGN.md`
- `docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/TODO.md`
- `docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/TASKS.jsonl`
- `docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CAMPAIGNS.jsonl`
- `docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/MILESTONES.md`
- `crates/jellyflow-runtime/src/io/tuning/pan_inertia.rs`
- `crates/jellyflow-runtime/src/runtime/events/viewport.rs`

## Evidence Log

### 2026-06-02 - JPIS-010 Workstream Opened

Scope: `docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1`

Result:

- Opened the pan inertia scheduling lane from closed viewport gesture and animation follow-ons.
- Set `JPIS-020` as the first executable task.
- Recorded renderer-free constraints from ADR 0001 and ADR 0003.
- Recorded existing `NodeGraphPanInertiaTuning` and `ViewportMoveKind::PanInertia` as source
  coverage.

Behavior proven:

- Planning artifacts agree on target state, gate set, task order, and autonomous commit policy.

Fresh verification:

- `jq empty docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CONTEXT.jsonl`: passed.
- `git diff --check`: passed.

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
