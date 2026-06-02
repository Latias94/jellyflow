# Jellyflow Viewport Pan Inertia Scheduling v1 - Milestones

Status: Closed
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

Exit criteria:

- Problem and target state are explicit.
- Existing tuning and event vocabulary are linked.
- Non-goals keep timers, velocity estimation, raw input, and renderer smoke outside runtime.
- First executable task is `JPIS-020`.

Status: complete on 2026-06-02.

## M1 - Pure Inertia Planner

Exit criteria:

- Runtime exposes renderer-neutral pan inertia request/plan/frame types.
- Initial velocity is finite, clamped, and converted from screen px/s to canvas pan by zoom.
- Disabled tuning, below-threshold speed, invalid damping, and invalid transforms reject
  deterministically.
- Focused runtime tests prove decay and stop behavior.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime pan_inertia
cargo nextest run -p jellyflow-runtime --test public_surface
```

Status: complete on 2026-06-02.

## M2 - Conformance And Template Integration

Exit criteria:

- Conformance fixtures can replay inertia frames through normal view-state publication.
- Adapter conformance tests cover accepted and rejected inertia scenarios.
- Template built-in suite includes pan inertia smoke before renderer tests.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

Status: complete on 2026-06-02.

## M3 - Documentation And Closeout

Exit criteria:

- README/runtime README explain what runtime owns versus adapter velocity/frame-loop ownership.
- Workstream evidence is current and machine-readable state is valid.
- Remaining platform-specific physics parity or renderer smoke work is split or deferred.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CONTEXT.jsonl
git diff --check
```

Status: complete on 2026-06-02.
