# Jellyflow Conformance Fixtures v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JCF-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-conformance-fixtures-v1]
  Goal: Open the conformance fixture workstream and freeze the first execution contract.
  Validation: DESIGN.md, TODO.md, MILESTONES.md, EVIDENCE_AND_GATES.md, CONTEXT.jsonl,
  WORKSTREAM.json, and HANDOFF.md exist and agree.
  Evidence: `docs/workstreams/jellyflow-conformance-fixtures-v1/DESIGN.md`
  Context: `docs/workstreams/jellyflow-conformance-fixtures-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Opened as the follow-on to the interaction harness and node drag kernel
  lanes.

## M1 - Fixture Vocabulary

- [x] JCF-020 [owner=codex] [deps=JCF-010] [scope=crates/jellyflow-runtime/src/runtime/conformance,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Define the first public headless conformance fixture vocabulary for graph setup, view/config
  setup, actions, gestures, and expected normalized trace events.
  Validation: `cargo nextest run -p jellyflow-runtime --test public_surface`;
  `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: Fixture types are public, serde-friendly where appropriate, renderer-free, and able to
  represent existing connect and node drag scenarios without renderer concepts.
  Context: `docs/workstreams/jellyflow-conformance-fixtures-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Added `runtime::conformance` scenario/setup/action/trace vocabulary,
  serde-enabled gesture/callback payloads needed by fixtures, and public surface round-trip
  coverage.

## M2 - Fixture Runner

- [x] JCF-030 [owner=codex] [deps=JCF-020] [scope=crates/jellyflow-runtime/src/runtime/conformance,crates/jellyflow-runtime/src/runtime/tests/conformance.rs]
  Goal: Add a headless fixture runner that executes scenarios against a real `NodeGraphStore` and
  returns compact normalized trace mismatches.
  Validation: `cargo nextest run -p jellyflow-runtime conformance`; `cargo check -p
  jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: Runner can execute at least one graph commit, one gesture, and one callback trace
  fixture with readable failure output.
  Context: `docs/workstreams/jellyflow-conformance-fixtures-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Added `run_conformance_scenario`, `ConformanceRunner`,
  `ConformanceRunReport`, and compact trace mismatch reporting; tests cover node drag gesture,
  graph commit, callback trace, and mismatch rendering.

## M3 - Convert Existing Scenarios

- [x] JCF-040 [owner=codex] [deps=JCF-030] [scope=crates/jellyflow-runtime/src/runtime/tests/adapter_conformance.rs,crates/jellyflow-runtime/src/runtime/tests/conformance.rs,crates/jellyflow-runtime/src/runtime/conformance]
  Goal: Convert existing connect and node drag adapter-conformance scenarios to use the fixture
  runner while preserving current behavior coverage.
  Validation: `cargo nextest run -p jellyflow-runtime adapter_conformance`; `cargo nextest run -p
  jellyflow-runtime conformance`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: Existing trace assertions still prove connect and drag ordering, transaction projection,
  gesture payloads, and callback payloads through the fixture runner.
  Context: `docs/workstreams/jellyflow-conformance-fixtures-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Converted connect dispatch, connect gesture lifecycle, connect gesture
  transaction callbacks, and node drag gesture callbacks to `run_conformance_scenario`; retained
  focused private harness tests for reconnect, delete, viewport, and geometry coverage.

## M4 - Documentation And Closeout

- [x] JCF-050 [owner=codex] [deps=JCF-040] [scope=docs/workstreams/jellyflow-conformance-fixtures-v1,README.md,crates/jellyflow-runtime/README.md]
  Goal: Document the fixture strategy, record fresh evidence, and close the lane or split follow-ons.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime`; `cargo clippy -p
  jellyflow-runtime --all-targets -- -D warnings`; `jq empty
  docs/workstreams/jellyflow-conformance-fixtures-v1/WORKSTREAM.json`; `git diff --check`.
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: README/runtime README explain how fixture conformance fits before renderer smoke tests.
  Context: `docs/workstreams/jellyflow-conformance-fixtures-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Updated README material, ran closeout gates, recorded closeout audit,
  and closed the workstream.
