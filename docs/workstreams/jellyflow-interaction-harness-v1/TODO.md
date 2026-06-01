# Jellyflow Interaction Harness v1 - TODO

Status: Active
Last updated: 2026-06-01

## M0 - Scope And Harness Contract

- [x] JIH-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-interaction-harness-v1]
  Goal: Open the interaction harness lane and freeze the first test-harness contract.
  Validation: DESIGN.md, TODO.md, MILESTONES.md, EVIDENCE_AND_GATES.md, CONTEXT.jsonl, WORKSTREAM.json, and HANDOFF.md exist and agree.
  Evidence: `docs/workstreams/jellyflow-interaction-harness-v1/DESIGN.md`
  Context: `docs/workstreams/jellyflow-interaction-harness-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Opened from ADR 0003 and the user's request for a mature automated harness that helps humans and agents catch interaction regressions.

## M1 - Runtime Trace Harness

- [x] JIH-020 [owner=codex] [deps=JIH-010] [scope=crates/jellyflow-runtime/src/runtime/tests/harness.rs,crates/jellyflow-runtime/src/runtime/tests.rs,crates/jellyflow-runtime/src/runtime/tests/adapter_conformance.rs]
  Goal: Add a reusable test-only harness around a real `NodeGraphStore` that records graph commit, view, and gesture traces with scenario-aware assertions.
  Validation: `cargo nextest run -p jellyflow-runtime adapter_conformance`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: A conformance scenario uses the harness to prove graph/view/gesture event ordering through observable runtime APIs.
  Context: `docs/workstreams/jellyflow-interaction-harness-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Added a private runtime test harness with normalized graph commit, view, and gesture traces; migrated adapter conformance scenarios to use scenario-aware trace assertions; targeted gates passed.

## M2 - Fixture-Driven Selection Kernel

- [ ] JIH-030 [owner=codex] [deps=JIH-020] [scope=crates/jellyflow-runtime/src/runtime/tests/**,crates/jellyflow-runtime/src/runtime/**]
  Goal: Add the first renderer-neutral selection-box fixture and, if needed, a minimal headless selection helper that turns a canvas box into ordered selection state.
  Validation: `cargo nextest run -p jellyflow-runtime selection`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: Fixture covers selectable policy, hidden nodes, sorted output, additive replacement semantics, and emitted selection events.
  Context: `docs/workstreams/jellyflow-interaction-harness-v1/CONTEXT.jsonl`

## M3 - Gesture Kernel Fixtures

- [ ] JIH-040 [owner=codex] [deps=JIH-030] [scope=crates/jellyflow-runtime/src/runtime/tests/**,crates/jellyflow-runtime/src/runtime/**]
  Goal: Extend the harness to cover at least one drag or connect/reconnect gesture fixture with expected transactions and callbacks.
  Validation: `cargo nextest run -p jellyflow-runtime adapter_conformance`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: Fixture trace proves pointer intent, graph transaction, view/gesture event order, and XyFlow projection where applicable.
  Context: `docs/workstreams/jellyflow-interaction-harness-v1/CONTEXT.jsonl`

## M4 - Closeout

- [ ] JIH-050 [owner=codex] [deps=JIH-040] [scope=docs/workstreams/jellyflow-interaction-harness-v1,README.md,crates/jellyflow-runtime/README.md]
  Goal: Document the harness strategy, record fresh evidence, and either close the lane or split follow-ons for public fixture APIs and renderer adapter smoke tests.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`; `git diff --check`.
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: EVIDENCE_AND_GATES.md and HANDOFF.md.
  Context: `docs/workstreams/jellyflow-interaction-harness-v1/CONTEXT.jsonl`
