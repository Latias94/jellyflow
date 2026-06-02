# Jellyflow Node Resize Kernel v1 - TODO

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

- [x] JNR-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-node-resize-kernel-v1,CONTEXT.md]
  Goal: Open the node resize kernel workstream from the XyFlow `XYResizer` source evidence and
  current Jellyflow `SetNodeSize` surface.
  Validation: jq empty docs/workstreams/jellyflow-node-resize-kernel-v1/WORKSTREAM.json
  docs/workstreams/jellyflow-node-resize-kernel-v1/TASKS.jsonl
  docs/workstreams/jellyflow-node-resize-kernel-v1/CAMPAIGNS.jsonl
  docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl; git diff --check
  Review: planner self-review for artifact agreement.
  Evidence: docs/workstreams/jellyflow-node-resize-kernel-v1/DESIGN.md.
  Context: docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Lane opened with JNR-020 as the first executable task.
  State: TASKS.jsonl entry JNR-010 matches this task.

## M1 - Minimal Pure Resize Planner

- [x] JNR-020 [owner=codex] [deps=JNR-010] [scope=crates/jellyflow-runtime/src/runtime,crates/jellyflow-runtime/src/runtime/tests]
  Goal: Add the first renderer-neutral node resize request/plan that emits deterministic
  `SetNodeSize` transactions for a single node with min/max bounds.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime resize; cargo nextest run -p
  jellyflow-runtime --test public_surface
  Review: review-workstream before accepting completion.
  Evidence: `crates/jellyflow-runtime/src/runtime/resize/`, `crates/jellyflow-runtime/src/runtime/tests/resize.rs`, and `crates/jellyflow-runtime/tests/public_surface.rs`.
  Context: docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Added a renderer-neutral single-node resize planner, store apply
  helper, min/max constraints, and public surface smoke.
  State: TASKS.jsonl entry JNR-020 records DONE with validation and evidence.

## M2 - Origin, Position, And Extent Constraints

- [x] JNR-030 [owner=codex] [deps=JNR-020] [scope=crates/jellyflow-runtime/src/runtime,crates/jellyflow-runtime/src/runtime/tests]
  Goal: Extend resize planning for left/top position changes, node origin, parent extents, and
  child extent restrictions where the contract is clear.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime resize; cargo nextest run -p
  jellyflow-runtime drag_parent_expansion
  Review: review-workstream before accepting completion.
  Evidence: `crates/jellyflow-runtime/src/runtime/resize/`, `crates/jellyflow-runtime/src/runtime/tests/resize.rs`, and `docs/workstreams/jellyflow-node-resize-kernel-v1/EVIDENCE_AND_GATES.md`.
  Context: docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Added XyFlow-style resize directions, position updates for left/top
  controls, node-origin fallback/override handling, and an evidence-backed split decision for
  parent/child extent and keep-aspect-ratio parity.
  State: TASKS.jsonl entry JNR-030 records DONE with validation, evidence, and split decision.

## M3 - Conformance And Template Integration

- [x] JNR-040 [owner=codex] [deps=JNR-030] [scope=crates/jellyflow-runtime/src/runtime/conformance,crates/jellyflow-runtime/src/runtime/tests/adapter_conformance,templates/headless-adapter]
  Goal: Add conformance/template coverage for resize transactions and adapter-facing callback
  traces once the resize planner interface stabilizes.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime conformance; cargo nextest
  run -p jellyflow-runtime adapter_conformance; cargo test --manifest-path
  templates/headless-adapter/Cargo.toml; cargo run --manifest-path templates/headless-adapter/Cargo.toml
  -- check
  Review: review-workstream before accepting completion.
  Evidence: `crates/jellyflow-runtime/src/runtime/conformance/scenario/action.rs`, `crates/jellyflow-runtime/src/runtime/conformance/runner/actions.rs`, `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/fixture_runner/node_resize.rs`, and `templates/headless-adapter/src/lib.rs`.
  Context: docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Added `apply_node_resize` fixture vocabulary, conformance runner
  support, adapter conformance trace coverage, and a template node resize smoke scenario.
  State: TASKS.jsonl entry JNR-040 records DONE with validation and evidence.

## M4 - Documentation And Closeout

- [ ] JNR-050 [owner=codex] [deps=JNR-040] [scope=README.md,crates/jellyflow-runtime/README.md,CONTEXT.md,docs/workstreams/jellyflow-node-resize-kernel-v1]
  Goal: Document resize runtime/adapter boundaries, record fresh evidence, and close or split
  follow-ons.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime; cargo clippy -p
  jellyflow-runtime --all-targets -- -D warnings; jq empty
  docs/workstreams/jellyflow-node-resize-kernel-v1/WORKSTREAM.json
  docs/workstreams/jellyflow-node-resize-kernel-v1/TASKS.jsonl
  docs/workstreams/jellyflow-node-resize-kernel-v1/CAMPAIGNS.jsonl
  docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl; git diff --check
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: EVIDENCE_AND_GATES.md, WORKSTREAM.json, and closeout audit.
  Context: docs/workstreams/jellyflow-node-resize-kernel-v1/CONTEXT.jsonl.
  Handoff: TODO.
  State: TASKS.jsonl entry JNR-050 must record closeout validation, evidence, and handoff status.
