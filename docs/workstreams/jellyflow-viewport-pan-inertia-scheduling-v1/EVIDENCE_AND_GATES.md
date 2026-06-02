# Jellyflow Viewport Pan Inertia Scheduling v1 - Evidence And Gates

Status: Closed
Last updated: 2026-06-02

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime
```

JPIS-020 and JPIS-030 are complete. JPIS-040 should now run package and closeout gates.

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
- `crates/jellyflow-runtime/src/runtime/conformance/scenario/action.rs`
- `crates/jellyflow-runtime/src/runtime/conformance/runner/actions.rs`
- `templates/headless-adapter/src/lib.rs`

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

### 2026-06-02 - JPIS-030 Conformance And Template Inertia Replay

Scope:

- `crates/jellyflow-runtime/src/runtime/conformance/scenario/action.rs`
- `crates/jellyflow-runtime/src/runtime/conformance/runner/actions.rs`
- `crates/jellyflow-runtime/src/runtime/tests/conformance/runner/viewport.rs`
- `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/fixture_runner/viewport.rs`
- `crates/jellyflow-runtime/tests/public_surface.rs`
- `templates/headless-adapter/src/lib.rs`
- `templates/headless-adapter/tests/conformance.rs`
- `templates/headless-adapter/README.md`

Result:

- Added conformance actions for applying one pan inertia frame, applying a frame sequence,
  asserting a sampled frame, and expecting a rejected inertia plan.
- Runner actions now call the pure pan inertia planner and publish accepted frames through
  `store.set_viewport`, preserving normal view-state and xyflow callback traces.
- Adapter conformance covers accepted frame replay, rejected below-threshold velocity, and the
  resulting viewport trace/callback ordering.
- The headless adapter template built-in suite now includes a fourth smoke scenario for sampled
  pan inertia before any renderer-specific smoke.

Behavior proven:

- Fixture runner rejects missing pan inertia frame sequences before planning.
- Valid inertia frame sequences publish `ViewChanged`, `ViewChange`, and `ViewportChange` traces.
- Rejected inertia plans can be asserted without producing trace events.
- Template CLI `check` reports four matching scenarios, including `template viewport pan inertia`.

Fresh verification:

- Red check: `cargo nextest run -p jellyflow-runtime adapter_conformance_fixture_runner_applies_viewport_pan_inertia_frames` failed before implementation because the conformance action constructors did not exist.
- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 3 tests run, 3 passed.
- `cargo nextest run -p jellyflow-runtime conformance`: passed, 51 tests run, 51 passed, 226 skipped.
- `cargo nextest run -p jellyflow-runtime adapter_conformance`: passed, 16 tests run, 16 passed, 261 skipped.
- `cargo test --manifest-path templates/headless-adapter/Cargo.toml`: passed, 7 tests run, 7 passed.
- `cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check`: passed, 4 matching scenarios.

### 2026-06-02 - JPIS-040 Documentation And Closeout

Scope:

- `README.md`
- `crates/jellyflow-runtime/README.md`
- `docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1`

Result:

- README/runtime README now document pan inertia as a runtime-owned pure planning contract.
- Documentation calls out adapter ownership of release velocity estimation, frame clocks,
  interruption/cancellation policy, sampled-frame commits, renderer smoke, screenshots, and pixel
  assertions.
- Workstream state is closed with renderer-specific follow-ons deferred outside this runtime lane.

Fresh verification:

- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime`: passed, 277 tests run, 277 passed.
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
- `jq empty docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CONTEXT.jsonl`: passed.
- `git diff --check`: passed.

Review:

- Workstream compliance: JPIS-010 through JPIS-040 are complete, the target state is met, and ADR
  0001/0003 renderer boundaries remain intact.
- Code quality: inertia planning and conformance replay remain deterministic, renderer-neutral, and
  covered through public runtime/conformance surfaces.
- Residual risks are split as follow-ons rather than kept in this closed lane.

Behavior proven:

- Pan inertia frames can be planned, conformance-replayed, asserted, and smoked through the external
  headless adapter template without runtime timers or renderer dependencies.
- README/runtime README now teach the runtime/adapter split for future wgpu, egui, Fret, or other
  adapters.

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
