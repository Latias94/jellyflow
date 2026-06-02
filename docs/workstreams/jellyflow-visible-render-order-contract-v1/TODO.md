# Jellyflow Visible Render Order Contract v1 - TODO

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

- [x] VRO-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-visible-render-order-contract-v1,CONTEXT.md]
  Goal: Open the visible render order contract workstream from the existing visible-node and
  render-order contracts, ADR 0003 renderer-boundary guidance, and current adapter template needs.
  Validation: jq empty docs/workstreams/jellyflow-visible-render-order-contract-v1/WORKSTREAM.json
  docs/workstreams/jellyflow-visible-render-order-contract-v1/TASKS.jsonl
  docs/workstreams/jellyflow-visible-render-order-contract-v1/CAMPAIGNS.jsonl
  docs/workstreams/jellyflow-visible-render-order-contract-v1/CONTEXT.jsonl; git diff --check
  Review: planner self-review for artifact agreement.
  Evidence: docs/workstreams/jellyflow-visible-render-order-contract-v1/DESIGN.md.
  Context: docs/workstreams/jellyflow-visible-render-order-contract-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Lane opened with VRO-020 as the first executable task.
  State: TASKS.jsonl entry VRO-010 matches this task.

## M1 - Runtime Visible Render Order Contract

- [ ] VRO-020 [owner=codex] [deps=VRO-010] [scope=crates/jellyflow-runtime/src/runtime,crates/jellyflow-runtime/src/runtime/tests,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Add a renderer-neutral visible node render order helper and store method that combine
  viewport culling with node draw order and selected-node elevation.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime visible_node_render_order;
  cargo nextest run -p jellyflow-runtime --test public_surface
  Review: review-workstream before accepting completion.
  Evidence: `runtime::rendering` helper, `NodeGraphStore` helper, rendering tests, public surface
  smoke.
  Context: docs/workstreams/jellyflow-visible-render-order-contract-v1/CONTEXT.jsonl.
  Handoff: Final status must be DONE, DONE_WITH_CONCERNS, BLOCKED, or NEEDS_CONTEXT.
  State: TASKS.jsonl entry VRO-020 records owner, scope, validation, evidence, and handoff status.

## M2 - Conformance And Template Coverage

- [ ] VRO-030 [owner=codex] [deps=VRO-020] [scope=crates/jellyflow-runtime/src/runtime/conformance,crates/jellyflow-runtime/src/runtime/tests,templates/headless-adapter]
  Goal: Add conformance and template smoke coverage that lets adapters assert ordered visible node
  ids before renderer-specific smoke tests.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime conformance; cargo
  nextest run -p jellyflow-runtime adapter_conformance; cargo test --manifest-path
  templates/headless-adapter/Cargo.toml; cargo run --manifest-path templates/headless-adapter/Cargo.toml
  -- check
  Review: review-workstream before accepting completion.
  Evidence: conformance action/runner tests, adapter conformance traces, template scenario.
  Context: docs/workstreams/jellyflow-visible-render-order-contract-v1/CONTEXT.jsonl.
  Handoff: Final status must be DONE, DONE_WITH_CONCERNS, BLOCKED, or NEEDS_CONTEXT.
  State: TASKS.jsonl entry VRO-030 records owner, scope, validation, evidence, and handoff status.

## M3 - Documentation And Closeout

- [ ] VRO-040 [owner=codex] [deps=VRO-030] [scope=README.md,crates/jellyflow-runtime/README.md,CONTEXT.md,docs/workstreams/jellyflow-visible-render-order-contract-v1]
  Goal: Document visible render order runtime/adapter boundaries, record fresh evidence, and close
  or split full render-plan follow-ons.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime; cargo clippy -p
  jellyflow-runtime --all-targets -- -D warnings; jq empty
  docs/workstreams/jellyflow-visible-render-order-contract-v1/WORKSTREAM.json
  docs/workstreams/jellyflow-visible-render-order-contract-v1/TASKS.jsonl
  docs/workstreams/jellyflow-visible-render-order-contract-v1/CAMPAIGNS.jsonl
  docs/workstreams/jellyflow-visible-render-order-contract-v1/CONTEXT.jsonl; git diff --check
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: README.md, crates/jellyflow-runtime/README.md, CONTEXT.md, EVIDENCE_AND_GATES.md,
  WORKSTREAM.json.
  Context: docs/workstreams/jellyflow-visible-render-order-contract-v1/CONTEXT.jsonl.
  Handoff: Split follow-ons if the scope expands into visible edge culling, full scene render
  plans, renderer smoke harnesses, or spatial indexing.
  State: TASKS.jsonl entry VRO-040 records closeout validation, evidence, and handoff status.
