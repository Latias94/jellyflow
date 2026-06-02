# Jellyflow Adapter Conformance Runner v1 - Handoff

Status: Closed
Last updated: 2026-06-01

## Current State

The workstream is closed. It was opened as a follow-on to the conformance fixture and auto-pan
lanes. Existing `ConformanceScenario` and `run_conformance_scenario` APIs can replay a single
scenario; this lane adds a public suite-level helper for adapter crates.

JACR-010 is complete: workstream docs, task ledger, campaign record, gates, and context manifest are
created.

JACR-020 is complete: `ConformanceSuite`, `ConformanceSuiteReport`, and `run_conformance_suite`
are public under `runtime::conformance`. Focused tests cover trace mismatch aggregation and action
execution errors without aborting later scenarios. Public-surface smoke coverage constructs,
serializes, deserializes, and runs a suite.

JACR-030 is complete: README/runtime README explain suite runners as the pre-render adapter
conformance layer, closeout evidence is recorded, and the workstream is closed.

## Next Task

None in this workstream. Follow-ons are split below.

## Decisions Since Opening

- Keep `jellyflow-runtime` renderer-free per ADR 0003.
- Reuse existing `ConformanceRunner` semantics.
- Do not add file-backed golden fixture loading in this lane.
- Do not add renderer/platform frame-loop runners in this lane.

## Validation To Run

- `cargo nextest run -p jellyflow-runtime conformance_suite`
- `cargo nextest run -p jellyflow-runtime --test public_surface`
- `cargo check -p jellyflow-runtime`
- `jq empty docs/workstreams/jellyflow-adapter-conformance-runner-v1/WORKSTREAM.json docs/workstreams/jellyflow-adapter-conformance-runner-v1/TASKS.jsonl docs/workstreams/jellyflow-adapter-conformance-runner-v1/CAMPAIGNS.jsonl`

## Evidence So Far

- 2026-06-01: `cargo nextest run -p jellyflow-runtime conformance_suite` passed, 2 tests.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime --test public_surface` passed, 3 tests.
- 2026-06-01: `cargo check -p jellyflow-runtime` passed.
- 2026-06-01: `cargo fmt --check` passed at closeout.
- 2026-06-01: `cargo nextest run -p jellyflow-runtime` passed, 167 tests run at closeout.
- 2026-06-01: `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings` passed at closeout.
- 2026-06-01: `jq empty docs/workstreams/jellyflow-adapter-conformance-runner-v1/WORKSTREAM.json docs/workstreams/jellyflow-adapter-conformance-runner-v1/TASKS.jsonl docs/workstreams/jellyflow-adapter-conformance-runner-v1/CAMPAIGNS.jsonl`
  passed at closeout.
- 2026-06-01: `git diff --check` passed at closeout.

## Follow-On Candidates

- File-backed golden fixture loader.
- External adapter crate templates.
- Renderer smoke-test helpers outside `jellyflow-runtime`.
