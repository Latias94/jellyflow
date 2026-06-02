# Jellyflow Auto-Pan Integration v1 - Handoff

Status: Closed
Last updated: 2026-06-01

## Current State

The workstream is closed. It was opened as a follow-on to the closed node drag and viewport
interaction lanes.
Jellyflow already has public `runtime::viewport` pan/zoom helpers and store publication; auto-pan
should feed that path rather than introduce a second viewport mutation mechanism.

JAI-010 is complete: workstream docs, task ledger, campaign record, gates, and context manifest are
created.

JAI-020 is complete: `runtime::auto_pan` now exposes workflow activation, frame request/plan/outcome
types, `compute_auto_pan`, and `NodeGraphStore::apply_auto_pan`. Focused tests cover direction,
policy, invalid/no-op frames, and store view-state publication. Public-surface smoke coverage uses
the new API.

JAI-030 is complete: conformance fixtures now support `apply_auto_pan`, and tests assert one
auto-pan frame through store view-state publication plus XyFlow-style view callbacks. Adapter
conformance also replays an auto-pan frame through the fixture runner.

JAI-040 is complete: README/runtime README explain auto-pan as deterministic headless frame math,
adapters retain frame scheduling/input capture ownership, closeout evidence is recorded, and the
workstream is closed.

## Next Task

None in this workstream. Follow-ons are split below.

## Decisions Since Opening

- Keep `jellyflow-runtime` renderer-free per ADR 0003.
- Treat auto-pan as deterministic frame math plus store publication, not as an adapter event loop.
- Reuse `NodeGraphAutoPanTuning::speed` and `margin`; avoid new persisted config until evidence
  shows it is required.
- Selection workflows can use the generic kernel directly until a dedicated persisted selection
  toggle is justified.

## Validation To Run

- `cargo nextest run -p jellyflow-runtime auto_pan`
- `cargo check -p jellyflow-runtime`
- `cargo nextest run -p jellyflow-runtime conformance`
- `cargo nextest run -p jellyflow-runtime adapter_conformance`
- `jq empty docs/workstreams/jellyflow-auto-pan-integration-v1/WORKSTREAM.json docs/workstreams/jellyflow-auto-pan-integration-v1/TASKS.jsonl docs/workstreams/jellyflow-auto-pan-integration-v1/CAMPAIGNS.jsonl`

## Evidence So Far

- 2026-06-01: `cargo nextest run -p jellyflow-runtime auto_pan` passed, 4 tests.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime --test public_surface` passed, 3 tests.
- 2026-06-01: `cargo check -p jellyflow-runtime` passed.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime conformance` passed, 14 tests.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime adapter_conformance` passed, 9 tests.
- 2026-06-01: `cargo check -p jellyflow-runtime` passed after JAI-030.
- 2026-06-01: `cargo fmt --check` passed at closeout.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime` passed, 165 tests run at closeout.
- 2026-06-01: `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings` passed at closeout.
- 2026-06-01: `jq empty docs/workstreams/jellyflow-auto-pan-integration-v1/WORKSTREAM.json docs/workstreams/jellyflow-auto-pan-integration-v1/TASKS.jsonl docs/workstreams/jellyflow-auto-pan-integration-v1/CAMPAIGNS.jsonl`
  passed at closeout.
- 2026-06-01: `git diff --check` passed at closeout.

## Follow-On Candidates

- Selection-specific persisted auto-pan toggle if adapter integration proves it is needed.
- Viewport animation/smoothing policy.
- Renderer adapter frame-loop helpers outside `jellyflow-runtime`.
