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

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
