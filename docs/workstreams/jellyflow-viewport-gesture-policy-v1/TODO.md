# Jellyflow Viewport Gesture Policy v1 - TODO

Status: Active
Last updated: 2026-06-01

## M0 - Scope And Evidence Freeze

- [x] JVGP-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-viewport-gesture-policy-v1]
  Goal: Open the viewport gesture policy workstream from the architecture report top recommendation.
  Validation: DESIGN.md, TODO.md, MILESTONES.md, EVIDENCE_AND_GATES.md, CONTEXT.jsonl, WORKSTREAM.json, TASKS.jsonl, and CAMPAIGNS.jsonl exist and agree.
  Review: planner self-review for artifact agreement.
  Evidence: docs/workstreams/jellyflow-viewport-gesture-policy-v1/DESIGN.md
  Context: docs/workstreams/jellyflow-viewport-gesture-policy-v1/CONTEXT.jsonl
  Handoff: DONE 2026-06-01. Lane is active with JVGP-020 as the first executable task.
  State: TASKS.jsonl entry JVGP-010 matches this task.

## M1 - Headless Policy Proof

- [x] JVGP-020 [owner=codex] [deps=JVGP-010] [scope=crates/jellyflow-runtime/src/runtime/viewport.rs,crates/jellyflow-runtime/src/runtime/tests/viewport.rs]
  Goal: Add the first headless viewport gesture policy slice for wheel/pinch and drag-pan gate decisions without changing existing viewport math behavior.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime viewport
  Review: review-workstream before accepting completion.
  Evidence: crates/jellyflow-runtime/src/runtime/viewport.rs; crates/jellyflow-runtime/src/runtime/tests/viewport.rs; EVIDENCE_AND_GATES.md
  Context: docs/workstreams/jellyflow-viewport-gesture-policy-v1/CONTEXT.jsonl plus ADR-0003 and XyFlow xypanzoom reference files.
  Handoff: DONE 2026-06-01. Added renderer-neutral viewport gesture policy types and pure resolvers for scroll/pinch and drag-pan decisions, with focused viewport tests.
  State: TASKS.jsonl entry JVGP-020 records owner, scope, validation, evidence, and handoff status.

## M2 - Conformance Integration

- [x] JVGP-030 [owner=codex] [deps=JVGP-020] [scope=crates/jellyflow-runtime/src/runtime/conformance,crates/jellyflow-runtime/src/runtime/tests/adapter_conformance]
  Goal: Let adapter conformance scenarios exercise viewport gesture policy decisions through renderer-neutral fixture actions.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime adapter_conformance; cargo nextest run -p jellyflow-runtime conformance
  Review: review-workstream before accepting completion.
  Evidence: crates/jellyflow-runtime/src/runtime/conformance; crates/jellyflow-runtime/src/runtime/tests/adapter_conformance; EVIDENCE_AND_GATES.md
  Context: docs/workstreams/jellyflow-viewport-gesture-policy-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-01. Added viewport gesture policy fixture actions and adapter conformance coverage for accepted scroll/drag-pan policy plus expected rejections.
  State: TASKS.jsonl entry JVGP-030 records owner, scope, validation, evidence, and handoff status.

## M3 - Public Surface And Closeout

- [x] JVGP-040 [owner=codex] [deps=JVGP-030] [scope=crates/jellyflow-runtime/tests/public_surface.rs,docs/workstreams/jellyflow-viewport-gesture-policy-v1]
  Goal: Cover exported policy types, run package gates, and close or split the lane.
  Validation: cargo fmt --check; cargo nextest run -p jellyflow-runtime --test public_surface; cargo nextest run -p jellyflow-runtime; cargo clippy -p jellyflow-runtime --all-targets -- -D warnings; jq empty docs/workstreams/jellyflow-viewport-gesture-policy-v1/WORKSTREAM.json docs/workstreams/jellyflow-viewport-gesture-policy-v1/TASKS.jsonl docs/workstreams/jellyflow-viewport-gesture-policy-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-viewport-gesture-policy-v1/CONTEXT.jsonl; git diff --check
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: EVIDENCE_AND_GATES.md; WORKSTREAM.json; CLOSEOUT_AUDIT_2026-06-01.md
  Context: docs/workstreams/jellyflow-viewport-gesture-policy-v1/CONTEXT.jsonl.
  Handoff: DONE 2026-06-01. Public surface smoke covers viewport gesture policy and conformance fixture vocabulary; package, clippy, metadata, and diff gates passed; lane closed.
  State: TASKS.jsonl entry JVGP-040 is verified or accepted before closeout.
