# Jellyflow Viewport Interaction Kernel v1 - TODO

Status: Active
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JVI-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-viewport-interaction-kernel-v1]
  Goal: Open the viewport interaction workstream and freeze the first execution contract.
  Validation: DESIGN.md, TODO.md, TASKS.jsonl, CAMPAIGNS.jsonl, MILESTONES.md,
  EVIDENCE_AND_GATES.md, CONTEXT.jsonl, WORKSTREAM.json, and HANDOFF.md exist and agree.
  Evidence: `docs/workstreams/jellyflow-viewport-interaction-kernel-v1/DESIGN.md`
  Context: `docs/workstreams/jellyflow-viewport-interaction-kernel-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Opened as the follow-on to conformance fixtures and node drag
  interaction kernels.

## M1 - Viewport Kernel

- [x] JVI-020 [owner=codex] [deps=JVI-010] [scope=crates/jellyflow-runtime/src/runtime/viewport,crates/jellyflow-runtime/src/runtime/tests/viewport.rs,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Add renderer-neutral viewport pan/zoom request types and deterministic transform helpers.
  Validation: `cargo nextest run -p jellyflow-runtime viewport`; `cargo check -p
  jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: Helpers can express drag-pan and zoom-around-pointer without renderer or platform
  events.
  Context: `docs/workstreams/jellyflow-viewport-interaction-kernel-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Added public `runtime::viewport` request/transform helpers,
  focused viewport tests, and public surface coverage.

## M2 - Store Gesture And Callbacks

- [ ] JVI-030 [owner=codex] [deps=JVI-020] [scope=crates/jellyflow-runtime/src/runtime/events,crates/jellyflow-runtime/src/runtime/store,crates/jellyflow-runtime/src/runtime/xyflow,crates/jellyflow-runtime/src/runtime/tests/viewport.rs]
  Goal: Wire viewport intent helpers through `NodeGraphStore` view-state publication and viewport
  gesture/callback lifecycle events.
  Validation: `cargo nextest run -p jellyflow-runtime viewport`; `cargo check -p
  jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: Tests prove view change emission, gesture ordering, and XyFlow-compatible move
  callbacks without renderer dependencies.
  Context: `docs/workstreams/jellyflow-viewport-interaction-kernel-v1/CONTEXT.jsonl`

## M3 - Conformance Fixtures

- [ ] JVI-040 [owner=codex] [deps=JVI-030] [scope=crates/jellyflow-runtime/src/runtime/tests/conformance.rs,crates/jellyflow-runtime/src/runtime/tests/adapter_conformance.rs,crates/jellyflow-runtime/src/runtime/conformance]
  Goal: Add viewport pan/zoom conformance fixtures and convert the viewport adapter-conformance
  trace to the fixture runner where appropriate.
  Validation: `cargo nextest run -p jellyflow-runtime conformance`; `cargo nextest run -p
  jellyflow-runtime adapter_conformance`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: Fixture traces prove viewport state changes, gesture payloads, and callback ordering.
  Context: `docs/workstreams/jellyflow-viewport-interaction-kernel-v1/CONTEXT.jsonl`

## M4 - Documentation And Closeout

- [ ] JVI-050 [owner=codex] [deps=JVI-040] [scope=docs/workstreams/jellyflow-viewport-interaction-kernel-v1,README.md,crates/jellyflow-runtime/README.md]
  Goal: Document viewport conformance, record fresh evidence, and close the lane or split
  follow-ons.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime`; `cargo clippy -p
  jellyflow-runtime --all-targets -- -D warnings`; `jq empty
  docs/workstreams/jellyflow-viewport-interaction-kernel-v1/WORKSTREAM.json`; `git diff --check`.
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: README/runtime README explain viewport headless conformance before renderer smoke tests.
  Context: `docs/workstreams/jellyflow-viewport-interaction-kernel-v1/CONTEXT.jsonl`
