# Jellyflow Viewport Pan Inertia Scheduling v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-02

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime conformance
```

JPIS-020 has added focused pan inertia tests. JPIS-030 should now prove conformance/template replay.

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

### 2026-06-02 - JPIS-020 Pure Inertia Planner

Scope:

- `crates/jellyflow-runtime/src/runtime/viewport/inertia.rs`
- `crates/jellyflow-runtime/src/runtime/viewport/mod.rs`
- `crates/jellyflow-runtime/src/runtime/tests/viewport/inertia.rs`
- `crates/jellyflow-runtime/src/runtime/tests/viewport/mod.rs`
- `crates/jellyflow-runtime/tests/public_surface.rs`

Result:

- Added renderer-neutral pan inertia request, plan, and frame types under `runtime::viewport`.
- Added a pure planner that consumes adapter-provided logical screen px/s release velocity and
  existing `NodeGraphPanInertiaTuning`.
- Exported the planner and types through the viewport public surface.

Behavior proven:

- Disabled tuning, below-threshold speed, invalid damping, invalid speed clamps, invalid velocity,
  and invalid viewport transforms reject deterministically.
- Initial release velocity is clamped to `max_speed` while preserving direction.
- Exponential decay sampling returns deterministic velocity, progress, screen speed, and transform
  frames.
- Screen displacement is converted into canvas pan by the current zoom.
- Terminal and late samples remain stable after the stop threshold.

Fresh verification:

- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime pan_inertia`: passed, 3 tests run, 3 passed, 270 skipped.
- `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 3 tests run, 3 passed.

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
