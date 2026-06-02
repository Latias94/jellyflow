# Jellyflow Visible Elements Contract v1 - TODO

Status: Closed
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

- [x] JVE-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-visible-elements-contract-v1,CONTEXT.md]
  Goal: Open the visible elements contract workstream from XyFlow `onlyRenderVisibleElements`
  source evidence, existing Jellyflow `get_nodes_inside`, and deferred spatial-index follow-ons.
  Validation: jq empty docs/workstreams/jellyflow-visible-elements-contract-v1/WORKSTREAM.json
  docs/workstreams/jellyflow-visible-elements-contract-v1/TASKS.jsonl
  docs/workstreams/jellyflow-visible-elements-contract-v1/CAMPAIGNS.jsonl
  docs/workstreams/jellyflow-visible-elements-contract-v1/CONTEXT.jsonl; git diff --check
  Review: planner self-review for artifact agreement.
  Evidence: docs/workstreams/jellyflow-visible-elements-contract-v1/DESIGN.md.
  Context: docs/workstreams/jellyflow-visible-elements-contract-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Lane opened with JVE-020 as the first executable task.
  State: TASKS.jsonl entry JVE-010 matches this task.

## M1 - Visible Node Runtime Contract

- [x] JVE-020 [owner=codex] [deps=JVE-010] [scope=crates/jellyflow-runtime/src/runtime,crates/jellyflow-runtime/src/runtime/tests,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Add a renderer-neutral visible node id planner and store helper using viewport transform,
  logical viewport size, node-origin policy, and `only_render_visible_elements`.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime visible_node; cargo nextest
  run -p jellyflow-runtime --test public_surface
  Review: review-workstream before accepting completion.
  Evidence: `runtime::rendering::VisibleNodeIdsRequest`, `resolve_visible_node_ids`,
  `NodeGraphStore::visible_node_ids`, rendering tests, public surface smoke.
  Context: docs/workstreams/jellyflow-visible-elements-contract-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Visible node ids landed in `runtime::rendering` beside render-order
  helpers, with deterministic linear lookup behavior and store tuning integration.
  State: TASKS.jsonl entry JVE-020 records DONE with validation and evidence.

## M2 - Conformance And Template Coverage

- [x] JVE-030 [owner=codex] [deps=JVE-020] [scope=crates/jellyflow-runtime/src/runtime/conformance,crates/jellyflow-runtime/src/runtime/tests,templates/headless-adapter]
  Goal: Add conformance and template smoke coverage that lets adapters assert visible node ids
  before renderer-specific smoke tests.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime conformance; cargo
  nextest run -p jellyflow-runtime adapter_conformance; cargo test --manifest-path
  templates/headless-adapter/Cargo.toml; cargo run --manifest-path templates/headless-adapter/Cargo.toml
  -- check
  Review: review-workstream before accepting completion.
  Evidence: conformance action/runner tests, adapter conformance traces, template scenario.
  Context: docs/workstreams/jellyflow-visible-elements-contract-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Added `assert_visible_node_ids` to conformance fixtures and the
  headless adapter template suite.
  State: TASKS.jsonl entry JVE-030 records DONE with validation and evidence.

## M3 - Documentation And Closeout

- [x] JVE-040 [owner=codex] [deps=JVE-030] [scope=README.md,crates/jellyflow-runtime/README.md,CONTEXT.md,docs/workstreams/jellyflow-visible-elements-contract-v1]
  Goal: Document visible node runtime/adapter boundaries, record fresh evidence, and close or split
  visible edge/spatial-index follow-ons.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime; cargo clippy -p
  jellyflow-runtime --all-targets -- -D warnings; jq empty
  docs/workstreams/jellyflow-visible-elements-contract-v1/WORKSTREAM.json
  docs/workstreams/jellyflow-visible-elements-contract-v1/TASKS.jsonl
  docs/workstreams/jellyflow-visible-elements-contract-v1/CAMPAIGNS.jsonl
  docs/workstreams/jellyflow-visible-elements-contract-v1/CONTEXT.jsonl; git diff --check
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: README.md, crates/jellyflow-runtime/README.md, CONTEXT.md, EVIDENCE_AND_GATES.md,
  WORKSTREAM.json, and closeout audit.
  Context: docs/workstreams/jellyflow-visible-elements-contract-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Runtime/docs closeout recorded visible-node boundaries and split
  visible edge culling plus real spatial indexing into follow-ons.
  State: TASKS.jsonl entry JVE-040 records closeout validation, evidence, and closed handoff.
