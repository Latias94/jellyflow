# Jellyflow Viewport Animation Scheduling v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-02

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime viewport_animation
```

This lane starts with a missing test filter; JVAS-020 should add the first focused tests with the
pure runtime animation planner.

## Gate Set

### Targeted Planner Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime viewport_animation
cargo nextest run -p jellyflow-runtime --test public_surface
```

### Double-Click Zoom Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime double_click_zoom
```

### Conformance Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
```

### Package And Closeout Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
```

### Metadata And Diff Gate

```bash
jq empty docs/workstreams/jellyflow-viewport-animation-scheduling-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-animation-scheduling-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CONTEXT.jsonl
git diff --check
```

## Evidence Anchors

- `docs/workstreams/jellyflow-viewport-animation-scheduling-v1/DESIGN.md`
- `docs/workstreams/jellyflow-viewport-animation-scheduling-v1/TODO.md`
- `docs/workstreams/jellyflow-viewport-animation-scheduling-v1/TASKS.jsonl`
- `docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CAMPAIGNS.jsonl`
- `docs/workstreams/jellyflow-viewport-animation-scheduling-v1/MILESTONES.md`
- runtime viewport animation tests added by JVAS-020
- runtime double-click zoom tests added by JVAS-030
- conformance tests added by JVAS-040

## Evidence Log

### 2026-06-02 - JVAS-010 Workstream Opened

Scope: `docs/workstreams/jellyflow-viewport-animation-scheduling-v1`

Result:

- Opened the viewport animation scheduling lane from closed viewport smoothing and double-click
  zoom follow-ons.
- Set `JVAS-020` as the first executable task.
- Recorded renderer-free constraints from ADR 0001 and ADR 0003.
- Recorded XyFlow panzoom duration/ease/interpolate references.

Behavior proven:

- Planning artifacts agree on target state, gate set, task order, and autonomous commit policy.

Fresh verification:

- `jq empty docs/workstreams/jellyflow-viewport-animation-scheduling-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-animation-scheduling-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CONTEXT.jsonl`: passed.
- `git diff --check`: passed.

### 2026-06-02 - JVAS-020 Pure Animation Planner

Scope:

- `crates/jellyflow-runtime/src/runtime/viewport/animation.rs`
- `crates/jellyflow-runtime/src/runtime/viewport/mod.rs`
- `crates/jellyflow-runtime/src/runtime/tests/viewport/animation.rs`
- `crates/jellyflow-runtime/tests/public_surface.rs`

Commands:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime viewport_animation
cargo nextest run -p jellyflow-runtime --test public_surface
cargo nextest run -p jellyflow-runtime viewport
```

Result:

- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime viewport_animation`: passed, 4 tests run, 4 passed.
- `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 3 tests run, 3 passed.
- `cargo nextest run -p jellyflow-runtime viewport`: passed, 34 tests run, 34 passed.

Behavior proven:

- Viewport animation plans sample deterministic cubic-in-out eased frames.
- Zero-duration animations finish immediately at the target transform.
- Non-finite durations and elapsed times are rejected; negative elapsed time clamps to the start.
- Linear easing is available as a named, serializable runtime option.
- Public surface smoke covers exported animation request/options/easing/plan/frame types.

### 2026-06-02 - JVAS-030 Double-Click Zoom Plan

Scope:

- `crates/jellyflow-runtime/src/runtime/viewport/gesture/double_click.rs`
- `crates/jellyflow-runtime/src/runtime/viewport/gesture/types.rs`
- `crates/jellyflow-runtime/src/runtime/viewport/gesture/mod.rs`
- `crates/jellyflow-runtime/src/runtime/viewport/mod.rs`
- `crates/jellyflow-runtime/src/runtime/tests/viewport/gesture_policy.rs`
- `crates/jellyflow-runtime/tests/public_surface.rs`

Commands:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime double_click_zoom
cargo nextest run -p jellyflow-runtime --test public_surface
cargo nextest run -p jellyflow-runtime viewport
```

Result:

- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime double_click_zoom`: passed, 2 tests run, 2 passed.
- `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 3 tests run, 3 passed.
- `cargo nextest run -p jellyflow-runtime viewport`: passed, 36 tests run, 36 passed.

Behavior proven:

- Normalized double-click zoom input resolves to an anchored viewport animation plan.
- The resolver respects `zoom_on_double_click`.
- Target zoom uses existing anchored zoom math and min/max clamps.
- Invalid factor and non-finite anchor inputs reject deterministically.
- Public surface smoke covers the new input and resolver.

### 2026-06-02 - JVAS-040 Conformance Trace Integration

Scope:

- `crates/jellyflow-runtime/src/runtime/conformance/scenario/action.rs`
- `crates/jellyflow-runtime/src/runtime/conformance/runner/actions.rs`
- `crates/jellyflow-runtime/src/runtime/tests/conformance/runner/viewport.rs`
- `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/fixture_runner/viewport.rs`
- `crates/jellyflow-runtime/tests/public_surface.rs`

Commands:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo nextest run -p jellyflow-runtime --test public_surface
```

Result:

- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime conformance`: passed, 43 tests run, 43 passed.
- `cargo nextest run -p jellyflow-runtime adapter_conformance`: passed, 14 tests run, 14 passed.
- `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 3 tests run, 3 passed.

Behavior proven:

- Conformance fixtures can assert sampled viewport animation frames without owning timers or
  renderer frame loops.
- Conformance fixtures can assert accepted double-click zoom animation plans against current
  runtime interaction policy.
- Conformance fixtures can assert expected double-click zoom rejections.
- Adapter conformance fixtures can use the same vocabulary while keeping expected render traces
  empty for pure planning checks.
- Public surface smoke covers the new serde-friendly conformance action vocabulary.

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
