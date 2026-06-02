# Jellyflow Adapter Template v1 - TODO

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

- [x] JAT-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-adapter-template-v1]
  Goal: Freeze the adapter template problem, renderer boundary, target state, and gates.
  Validation: `jq empty docs/workstreams/jellyflow-adapter-template-v1/WORKSTREAM.json docs/workstreams/jellyflow-adapter-template-v1/TASKS.jsonl docs/workstreams/jellyflow-adapter-template-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-adapter-template-v1/CONTEXT.jsonl`
  Review: planner self-review for artifact agreement.
  Evidence: `docs/workstreams/jellyflow-adapter-template-v1/DESIGN.md`
  Handoff: DONE 2026-06-02. Workstream opened from JACR follow-ons with ADR 0003 renderer-free
  constraints.

## M1 - Copyable Headless Adapter Template

- [x] JAT-020 [owner=codex] [deps=JAT-010] [scope=templates/headless-adapter]
  Goal: Add a non-workspace template crate that runs a headless adapter conformance suite through
  public Jellyflow APIs.
  Validation: `cargo test --manifest-path templates/headless-adapter/Cargo.toml`;
  `cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check`
  Review: code review for public API usage, renderer-free dependencies, and copyable structure.
  Evidence: `templates/headless-adapter`
  Context: `docs/workstreams/jellyflow-adapter-template-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-02. Added `templates/headless-adapter` with built-in node drag and
  viewport pan conformance scenarios, CLI check/approve commands, and template tests.

## M2 - Template Smoke Integration And Docs

- [x] JAT-030 [owner=codex] [deps=JAT-020] [scope=tools,README.md,crates/jellyflow-runtime/README.md]
  Goal: Wire the template into external smoke checks and document it as the starting point for
  wgpu, egui, Fret, or other adapter crates.
  Validation: `python3 tools/check_external_consumer_smoke.py`;
  `python3 tools/check_no_fret_dependencies.py`; `cargo fmt --check`
  Review: ensure smoke checks do not rely on workspace-only behavior and do not add renderer deps.
  Evidence: tool output and README updates.
  Context: `docs/workstreams/jellyflow-adapter-template-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-02. External smoke now runs the template with `cargo --manifest-path`,
  checks its cargo tree for Fret packages, and README material points adapters at the template.

## M3 - Closeout

- [ ] JAT-040 [owner=codex] [deps=JAT-030] [scope=docs/workstreams/jellyflow-adapter-template-v1]
  Goal: Record final evidence, close the lane, and split remaining adapter or renderer follow-ons.
  Validation: `cargo fmt --check`; `cargo nextest run --workspace`; template cargo gates;
  external smoke/no-Fret gates; `jq empty` for workstream JSON/JSONL; `git diff --check`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: `docs/workstreams/jellyflow-adapter-template-v1/EVIDENCE_AND_GATES.md`
  Handoff: Summarize next adapter-specific work in `HANDOFF.md`.
