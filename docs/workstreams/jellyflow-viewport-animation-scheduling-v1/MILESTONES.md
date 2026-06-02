# Jellyflow Viewport Animation Scheduling v1 - Milestones

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

Exit criteria:

- Problem and target state are explicit.
- Non-goals keep timers, event loops, renderer smoke, and raw platform input outside runtime.
- Relevant ADRs, prior lanes, and XyFlow panzoom references are linked.
- First executable task is `JVAS-020`.

Status: complete on 2026-06-02.

## M1 - Pure Animation Planner

Exit criteria:

- Runtime exposes renderer-neutral animation request/plan/frame types.
- Frame sampling is deterministic and validates finite transforms.
- Zero-duration behavior is immediate.
- Focused runtime tests cover easing, interpolation, and invalid input.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime viewport_animation
cargo nextest run -p jellyflow-runtime --test public_surface
```

Status: complete on 2026-06-02.

## M2 - Double-Click Zoom Plan

Exit criteria:

- Normalized double-click zoom input maps to an anchored animation plan.
- Disabled `zoom_on_double_click` and invalid input reject deterministically.
- Existing anchored zoom math remains the single source for target transform calculation.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime double_click_zoom
```

## M3 - Conformance Trace Integration

Exit criteria:

- Fixture vocabulary can describe viewport animation or double-click zoom planning without timers.
- Adapter conformance tests can assert accepted plans or expected rejections.
- Runtime stays renderer-free.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
```

## M4 - Documentation And Closeout

Exit criteria:

- README/runtime README explain what runtime owns versus adapter frame loops.
- Workstream evidence is current and machine-readable state is valid.
- Remaining inertia, exact d3 smooth interpolation, or renderer smoke work is split or deferred.

Primary gates:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-viewport-animation-scheduling-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-animation-scheduling-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-animation-scheduling-v1/CONTEXT.jsonl
git diff --check
```
