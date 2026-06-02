# Jellyflow Node Drag Parent Expansion v1 - TODO

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

- [x] JNPE-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-node-drag-parent-expansion-v1,CONTEXT.md]
  Goal: Open the node drag parent expansion workstream from the closed node drag follow-ons and
  XyFlow `expandParent` source evidence.
  Validation: jq empty docs/workstreams/jellyflow-node-drag-parent-expansion-v1/WORKSTREAM.json
  docs/workstreams/jellyflow-node-drag-parent-expansion-v1/TASKS.jsonl
  docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CAMPAIGNS.jsonl
  docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl; git diff --check
  Review: planner self-review for artifact agreement.
  Evidence: docs/workstreams/jellyflow-node-drag-parent-expansion-v1/DESIGN.md.
  Context: docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Lane opened with JNPE-020 as the first executable task.
  State: TASKS.jsonl entry JNPE-010 matches this task.

## M1 - Single-Parent Drag Expansion

- [x] JNPE-020 [owner=codex] [deps=JNPE-010] [scope=crates/jellyflow-runtime/src/runtime/drag,crates/jellyflow-runtime/src/runtime/tests/drag]
  Goal: Implement the minimal runtime drag planner behavior for one dragged child expanding one
  parent group while preserving `expand_parent = false` clamping.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime drag_parent_expansion;
  cargo nextest run -p jellyflow-runtime drag
  Review: review-workstream before accepting completion.
  Evidence: focused runtime drag tests and transaction op assertions.
  Context: docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Single-parent expansion now emits `SetNodePos` plus deterministic
  `SetGroupRect`; `expand_parent = false` keeps the existing parent extent clamp.
  State: TASKS.jsonl entry JNPE-020 records completion, validation, evidence, and handoff status.

## M2 - Multi-Selection And Sibling Compensation

- [x] JNPE-030 [owner=codex] [deps=JNPE-020] [scope=crates/jellyflow-runtime/src/runtime/drag,crates/jellyflow-runtime/src/runtime/tests/drag]
  Goal: Make parent expansion deterministic for multi-node drags, multiple parent groups, and
  non-dragged sibling compensation when parent rects expand left or upward.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime drag_parent_expansion;
  cargo nextest run -p jellyflow-runtime drag
  Review: review-workstream before accepting completion.
  Evidence: multi-parent and sibling-compensation runtime tests.
  Context: docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Multi-parent expansion order is deterministic; left/top expansion keeps
  non-dragged sibling node positions unchanged because Jellyflow stores node positions in canvas
  space.
  State: TASKS.jsonl entry JNPE-030 records completion, validation, evidence, and handoff status.

## M3 - Conformance And Adapter Trace Coverage

- [x] JNPE-040 [owner=codex] [deps=JNPE-030] [scope=crates/jellyflow-runtime/src/runtime/conformance,crates/jellyflow-runtime/src/runtime/tests/adapter_conformance,templates/headless-adapter]
  Goal: Add conformance/template coverage for parent expansion transactions and any adapter-facing
  node-change or callback traces affected by expanded parent groups.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime conformance; cargo nextest
  run -p jellyflow-runtime adapter_conformance; cargo test --manifest-path
  templates/headless-adapter/Cargo.toml; cargo run --manifest-path templates/headless-adapter/Cargo.toml
  -- check
  Review: review-workstream before accepting completion.
  Evidence: conformance/template parent expansion fixtures or a documented no-schema-change decision.
  Context: docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. `ApplyNodeDrag` already covers the runtime boundary, so no fixture
  schema change was needed; runner, adapter conformance, and template smoke now cover
  `set_group_rect` traces.
  State: TASKS.jsonl entry JNPE-040 records completion, validation, evidence, and handoff status.

## M4 - Documentation And Closeout

- [ ] JNPE-050 [owner=codex] [deps=JNPE-040] [scope=README.md,crates/jellyflow-runtime/README.md,docs/workstreams/jellyflow-node-drag-parent-expansion-v1]
  Goal: Document parent expansion boundaries, record fresh evidence, and close or split follow-ons.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime; cargo clippy -p
  jellyflow-runtime --all-targets -- -D warnings; jq empty
  docs/workstreams/jellyflow-node-drag-parent-expansion-v1/WORKSTREAM.json
  docs/workstreams/jellyflow-node-drag-parent-expansion-v1/TASKS.jsonl
  docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CAMPAIGNS.jsonl
  docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl; git diff --check
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: EVIDENCE_AND_GATES.md, WORKSTREAM.json, and closeout audit.
  Context: docs/workstreams/jellyflow-node-drag-parent-expansion-v1/CONTEXT.jsonl.
  Handoff: TODO.
  State: TASKS.jsonl entry JNPE-050 must record closeout validation, evidence, and handoff status.
