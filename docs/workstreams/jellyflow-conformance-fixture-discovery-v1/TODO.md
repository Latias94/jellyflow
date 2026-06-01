# Jellyflow Conformance Fixture Discovery v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- [x] JCFD-010 [owner=codex] [scope=docs/workstreams/jellyflow-conformance-fixture-discovery-v1]
  Goal: Open the fixture discovery workstream from file-backed suite follow-ons.
  Validation: `jq empty docs/workstreams/jellyflow-conformance-fixture-discovery-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-fixture-discovery-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-fixture-discovery-v1/CAMPAIGNS.jsonl`
  Review: planner self-review for artifact agreement.
  Evidence: Workstream docs, task ledger, context manifest, and gates are created.

## M1 - Directory Discovery

- [x] JCFD-020 [owner=codex] [deps=JCFD-010] [scope=crates/jellyflow-runtime/src/runtime/conformance/mod.rs,crates/jellyflow-runtime/src/runtime/tests/conformance.rs,crates/jellyflow-runtime/tests/public_surface.rs]
  Goal: Add deterministic recursive directory discovery for JSON conformance suite files.
  Validation: `cargo nextest run -p jellyflow-runtime conformance_fixture_directory`; `cargo nextest run -p jellyflow-runtime --test public_surface`; `cargo check -p jellyflow-runtime`
  Review: review-workstream before accepting completion.
  Evidence: Directory discovery tests and public-surface smoke coverage.
  Handoff: DONE 2026-06-01. Added `ConformanceFixtureDirectory`, `ConformanceSuiteFile`,
  path-aware directory reports, deterministic recursive JSON discovery, focused tests, and
  public-surface coverage.

## M2 - Documentation And Closeout

- [x] JCFD-030 [owner=codex] [deps=JCFD-020] [scope=README.md,crates/jellyflow-runtime/README.md,docs/workstreams/jellyflow-conformance-fixture-discovery-v1]
  Goal: Document directory-backed fixture discovery, record fresh evidence, and close the lane or
  split follow-ons.
  Validation: `cargo fmt --check`; `cargo nextest run -p jellyflow-runtime`; `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`; `jq empty docs/workstreams/jellyflow-conformance-fixture-discovery-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-fixture-discovery-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-fixture-discovery-v1/CAMPAIGNS.jsonl`; `git diff --check`
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: README/runtime README explain directory discovery before approval or renderer tests.
  Handoff: DONE 2026-06-01. Updated README/runtime README, recorded closeout audit and final
  verification evidence, closed the workstream, and split golden approval/update plus renderer
  golden assets as follow-ons.
