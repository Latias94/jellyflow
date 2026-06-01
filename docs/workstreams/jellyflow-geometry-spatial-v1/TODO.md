# Jellyflow Geometry Spatial v1 - TODO

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Evidence Freeze

- [x] JGS-010 [owner=codex] [deps=none] [scope=docs/workstreams/jellyflow-geometry-spatial-v1]
  Goal: Open the geometry/spatial fearless-refactor lane, freeze the problem statement, and identify the first executable slice.
  Validation: DESIGN.md, TODO.md, MILESTONES.md, EVIDENCE_AND_GATES.md, CONTEXT.jsonl, WORKSTREAM.json, and HANDOFF.md exist and agree.
  Evidence: `docs/workstreams/jellyflow-geometry-spatial-v1/DESIGN.md`
  Context: `docs/workstreams/jellyflow-geometry-spatial-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Opened from the top fearless-refactor recommendation after confirming the workspace test baseline passes.

## M1 - Shared Geometry Proof

- [x] JGS-020 [owner=codex] [deps=JGS-010] [scope=crates/jellyflow-runtime/src/runtime/geometry.rs,crates/jellyflow-runtime/src/runtime/geometry/**,crates/jellyflow-runtime/src/runtime/fit_view/**,crates/jellyflow-runtime/src/runtime/utils/**,tests]
  Goal: Introduce a shared runtime geometry module for canvas bounds, node-origin projection, rect union/intersection, and viewport target math; route existing fit-view and runtime utility helpers through it without changing public behavior.
  Validation: `cargo nextest run -p jellyflow-runtime runtime::fit_view`; `cargo nextest run -p jellyflow-runtime runtime::tests::utils`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: tests proving old fit-view and bounds behavior still pass through the shared primitives.
  Context: `docs/workstreams/jellyflow-geometry-spatial-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Added the shared `runtime::geometry` module, routed fit-view and runtime bounds helpers through it, and passed the required targeted gates plus the new `runtime::geometry` tests.

## M2 - Derived Layout And Spatial Queries

- [x] JGS-030 [owner=codex] [deps=JGS-020] [scope=crates/jellyflow-runtime/src/runtime/lookups/**,crates/jellyflow-runtime/src/runtime/utils/**,crates/jellyflow-runtime/src/io/tuning/spatial_index.rs,tests]
  Goal: Deepen lookup-derived geometry with parent/child lookup and deterministic node bounds/inside queries, then decide whether `NodeGraphSpatialIndexTuning` backs a real index in this lane or is deferred.
  Validation: `cargo nextest run -p jellyflow-runtime runtime::tests::lookups`; `cargo nextest run -p jellyflow-runtime runtime::tests::utils`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: tests for parent lookup, hidden nodes, missing sizes, deterministic ordering, and linear fallback behavior.
  Context: `docs/workstreams/jellyflow-geometry-spatial-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Added sorted parent/child lookup helpers derived from `node_lookup`, strengthened node-inside tests for hidden/missing-size/deterministic linear scan behavior, and deferred a real spatial-index backend as a follow-on.

- [x] JGS-040 [owner=codex] [deps=JGS-030] [scope=crates/jellyflow-runtime/src/runtime/geometry/**,crates/jellyflow-runtime/src/runtime/lookups/**,tests]
  Goal: Add renderer-neutral handle/edge endpoint geometry sufficient for adapters to compute source/target positions without DOM-specific measurement code.
  Validation: `cargo nextest run -p jellyflow-runtime edge_position`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: tests mirroring XyFlow handle-position cases with Jellyflow IDs and canvas geometry.
  Context: `docs/workstreams/jellyflow-geometry-spatial-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Published `runtime::geometry` endpoint primitives with node-local handle bounds, side anchors, center calculation, and edge source/target endpoint output; added lookup access to edge connections.

- [x] JGS-050 [owner=codex] [deps=JGS-040] [scope=crates/jellyflow-runtime/src/runtime/geometry/**,crates/jellyflow-runtime/src/io/config/**,tests]
  Goal: Add edge path and hit-test primitives only if the endpoint geometry contract is stable; otherwise split this as a follow-on.
  Validation: `cargo nextest run -p jellyflow-runtime edge_path`; `cargo nextest run -p jellyflow-runtime hit_test`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: deterministic path/hit-test fixtures for straight, bezier, and smoothstep-like edges or a documented follow-on split.
  Context: `docs/workstreams/jellyflow-geometry-spatial-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Added renderer-neutral path commands for straight, bezier, and smoothstep-like edges, numeric path distance hit testing, and config-derived hit-test options.

## M3 - Integration And Docs

- [x] JGS-060 [owner=codex] [deps=JGS-020] [scope=README.md,crates/jellyflow-runtime/README.md,crates/jellyflow-runtime/examples,docs/workstreams/jellyflow-geometry-spatial-v1]
  Goal: Document the shipped geometry/spatial surface, update examples if public paths change, and record fresh task evidence.
  Validation: `cargo nextest run -p jellyflow-runtime`; `cargo check -p jellyflow-runtime`.
  Review: review-workstream before accepting completion.
  Evidence: README/example updates and EVIDENCE_AND_GATES.md evidence log.
  Context: `docs/workstreams/jellyflow-geometry-spatial-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Updated root/runtime READMEs, added the `geometry_edge` runtime example, and passed runtime package gates.

## M4 - Closeout

- [x] JGS-070 [owner=codex] [deps=JGS-060] [scope=docs/workstreams/jellyflow-geometry-spatial-v1]
  Goal: Close the lane or split remaining geometry/spatial/interaction work into narrower follow-ons.
  Validation: `cargo fmt --check`; `cargo nextest run --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `python3 tools/check_no_fret_dependencies.py`; `python3 tools/check_external_consumer_smoke.py`; `jq empty docs/workstreams/jellyflow-geometry-spatial-v1/WORKSTREAM.json`; `git diff --check`.
  Review: review-workstream and verify-rust-workstream before closeout.
  Evidence: EVIDENCE_AND_GATES.md, WORKSTREAM.json, HANDOFF.md, and optional closeout audit.
  Context: `docs/workstreams/jellyflow-geometry-spatial-v1/CONTEXT.jsonl`
  Handoff: DONE 2026-06-01. Closed the lane after full workspace verification; split real spatial indexing and full interaction kernels as follow-ons.
