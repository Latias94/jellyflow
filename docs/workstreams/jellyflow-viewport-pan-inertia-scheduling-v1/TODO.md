# Jellyflow Viewport Pan Inertia Scheduling v1 - TODO

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

- [x] JPIS-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1]
  Goal: Open the pan inertia scheduling workstream from smooth viewport follow-ons.
  Validation: DESIGN.md, TODO.md, MILESTONES.md, EVIDENCE_AND_GATES.md, CONTEXT.jsonl,
  WORKSTREAM.json, TASKS.jsonl, and CAMPAIGNS.jsonl exist and agree.
  Review: planner self-review for artifact agreement.
  Evidence: docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/DESIGN.md
  Context: docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Lane opened with JPIS-020 as the first executable task.
  State: TASKS.jsonl entry JPIS-010 matches this task.

## M1 - Pure Inertia Planner

- [x] JPIS-020 [owner=codex] [deps=JPIS-010] [scope=crates/jellyflow-runtime/src/runtime/viewport,crates/jellyflow-runtime/src/runtime/tests/viewport,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Add renderer-neutral pan inertia request/plan/frame primitives that convert normalized
  screen velocity and tuning into deterministic viewport pan frames.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime pan_inertia; cargo
  nextest run -p jellyflow-runtime --test public_surface
  Review: review-workstream before accepting completion.
  Evidence: focused viewport pan inertia tests and public surface smoke.
  Context: docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Runtime now exposes a pure pan inertia planner; keep velocity
  estimation, timers, cancellation, and renderer smoke in adapters.
  State: TASKS.jsonl entry JPIS-020 records completion, validation, evidence, and handoff status.

## M2 - Conformance And Template Integration

- [x] JPIS-030 [owner=codex] [deps=JPIS-020] [scope=crates/jellyflow-runtime/src/runtime/conformance,crates/jellyflow-runtime/src/runtime/tests/adapter_conformance,templates/headless-adapter]
  Goal: Let conformance fixtures and the headless adapter template replay sampled inertia frames
  through the normal view-state publication path.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime conformance; cargo nextest
  run -p jellyflow-runtime adapter_conformance; cargo test --manifest-path
  templates/headless-adapter/Cargo.toml; cargo run --manifest-path templates/headless-adapter/Cargo.toml
  -- check
  Review: review-workstream before accepting completion.
  Evidence: conformance/template inertia traces.
  Context: docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-02. Conformance fixtures and the adapter template now replay sampled
  inertia frames through view-state publication while frame loops remain outside runtime.
  State: TASKS.jsonl entry JPIS-030 records completion, validation, evidence, and handoff status.

## M3 - Documentation And Closeout

- [ ] JPIS-040 [owner=codex] [deps=JPIS-030] [scope=README.md,crates/jellyflow-runtime/README.md,docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1]
  Goal: Document pan inertia scheduling boundaries, record fresh evidence, and close or split
  follow-ons.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime; cargo clippy -p
  jellyflow-runtime --all-targets -- -D warnings; jq empty
  docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/WORKSTREAM.json
  docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/TASKS.jsonl
  docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CAMPAIGNS.jsonl
  docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CONTEXT.jsonl; git diff --check
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: EVIDENCE_AND_GATES.md, WORKSTREAM.json, and closeout audit.
  Context: docs/workstreams/jellyflow-viewport-pan-inertia-scheduling-v1/CONTEXT.jsonl.
  Handoff: Summarize residual risks and follow-ons.
  State: TASKS.jsonl entry JPIS-040 is verified or accepted before closeout.
