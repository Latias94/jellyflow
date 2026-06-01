# Jellyflow Viewport Gesture Policy v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-01

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime viewport
```

## Gate Set

### Targeted Iteration Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime viewport
```

### Conformance Gate

```bash
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo nextest run -p jellyflow-runtime conformance
```

### Public Surface Gate

```bash
cargo nextest run -p jellyflow-runtime --test public_surface
```

### Package Gate

```bash
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
```

### Metadata And Diff Gate

```bash
jq empty docs/workstreams/jellyflow-viewport-gesture-policy-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-gesture-policy-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-gesture-policy-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-gesture-policy-v1/CONTEXT.jsonl
git diff --check
```

## Evidence Anchors

- `docs/workstreams/jellyflow-viewport-gesture-policy-v1/DESIGN.md`
- `docs/workstreams/jellyflow-viewport-gesture-policy-v1/TODO.md`
- `docs/workstreams/jellyflow-viewport-gesture-policy-v1/TASKS.jsonl`
- `docs/workstreams/jellyflow-viewport-gesture-policy-v1/CAMPAIGNS.jsonl`
- `docs/workstreams/jellyflow-viewport-gesture-policy-v1/MILESTONES.md`
- `crates/jellyflow-runtime/src/runtime/viewport.rs`
- `crates/jellyflow-runtime/src/runtime/tests/viewport.rs`

## Evidence Log

### 2026-06-01 - JVGP-010 Workstream Opened

Scope: `docs/workstreams/jellyflow-viewport-gesture-policy-v1`

Result:

- Opened the lane from the architecture report top recommendation.
- Set `JVGP-020` as the first executable task.
- Recorded renderer-free constraints from ADR-0001 and ADR-0003.

Behavior proven:

- Planning artifacts agree on target state, gate set, task order, and autonomous commit policy.

Fresh verification:

- Pending metadata gate after initial file creation.

### 2026-06-01 - JVGP-020 Headless Policy Proof

Scope:

- `crates/jellyflow-runtime/src/runtime/viewport.rs`
- `crates/jellyflow-runtime/src/runtime/tests/viewport.rs`

Commands:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime viewport
```

Result:

- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime viewport`: passed, 20 tests run, 20 passed.

Behavior proven:

- Normalized scroll input maps to pan-on-scroll screen deltas with mode and speed applied.
- Zoom activation key and Ctrl/pinch priority match XyFlow-style pan/zoom ordering.
- Drag-pan respects allowed pointer buttons and rejects during connection or user selection.
- Existing viewport pan/zoom math and gesture callback ordering remained covered.

### 2026-06-01 - JVGP-030 Conformance Integration

Scope:

- `crates/jellyflow-runtime/src/runtime/conformance/scenario/action.rs`
- `crates/jellyflow-runtime/src/runtime/conformance/runner/actions.rs`
- `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/fixture_runner.rs`

Commands:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo nextest run -p jellyflow-runtime conformance
```

Result:

- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime adapter_conformance`: passed, 11 tests run, 11 passed.
- `cargo nextest run -p jellyflow-runtime conformance`: passed, 28 tests run, 28 passed.

Behavior proven:

- Conformance fixtures can execute accepted viewport scroll and drag-pan gesture policy actions.
- Accepted gesture policy actions apply through the normal store viewport path and emit viewport traces.
- Expected gesture rejections are checked inside the fixture action without mutating view state.

### 2026-06-01 - JVGP-040 Public Surface And Closeout

Scope:

- `crates/jellyflow-runtime/tests/public_surface.rs`
- `docs/workstreams/jellyflow-viewport-gesture-policy-v1`

Commands:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime --test public_surface
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-viewport-gesture-policy-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-gesture-policy-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-gesture-policy-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-gesture-policy-v1/CONTEXT.jsonl
git diff --check
```

Result:

- `cargo fmt --check`: passed.
- `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 3 tests run, 3 passed.
- `cargo nextest run -p jellyflow-runtime`: passed, 183 tests run, 183 passed.
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
- `jq empty ...`: passed.
- `git diff --check`: passed.

Behavior proven:

- Public runtime surface exposes viewport gesture policy and fixture vocabulary.
- Runtime package behavior remains green after policy and conformance integration.
- Workstream metadata is valid and the diff has no whitespace errors.

## Notes

Fresh verification is required before marking any implementation task or the lane complete.
