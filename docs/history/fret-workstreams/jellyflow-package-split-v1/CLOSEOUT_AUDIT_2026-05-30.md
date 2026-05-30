# Jellyflow Package Split v1 Closeout Audit

Status: Closed
Last updated: 2026-05-30

## Outcome

This lane is closed. The in-workspace Jellyflow package boundary is proven far enough to stop this
execution lane and split future work into narrower follow-ons.

Completed boundaries:

- `jellyflow-core` owns the headless graph model, IDs, type descriptors, interaction value types,
  transaction ops, and history helpers.
- `jellyflow-runtime` owns headless I/O/view-state payloads, rules, schema/profile pipeline,
  store/apply/callback/controlled-mode substrate, fit-view helpers, and XyFlow-style utilities.
- `fret-node` remains the Fret adapter and compatibility facade.
- `jellyflow-geometry` is not extracted in this lane. JF-030 concluded the current geometry/spatial
  surface is still adapter-bound.

## Evidence

- ADR boundary: `docs/adr/0331-jellyflow-headless-node-graph-engine-boundary.md`
- Core crate: `ecosystem/jellyflow-core/`
- Runtime crate: `ecosystem/jellyflow-runtime/`
- Adapter facade: `ecosystem/fret-node/`
- Geometry decision: `docs/workstreams/jellyflow-package-split-v1/JF-030_GEOMETRY_SPATIAL_AUDIT_2026-05-30.md`
- Gate log: `docs/workstreams/jellyflow-package-split-v1/EVIDENCE_AND_GATES.md`

## Closeout Verification

- `cargo check -p jellyflow-runtime`: passed.
- `cargo check -p fret-node --all-features --tests`: passed.
- `python3 tools/check_layering.py`: passed.
- `jq empty docs/workstreams/jellyflow-package-split-v1/WORKSTREAM.json`: passed.
- `python3 tools/check_workstream_catalog.py`: passed, validating 510 dedicated directories and 47
  standalone markdown files.
- `git diff --check`: passed.

Broader workspace tests were not rerun during closeout because the closeout change is docs-only and
the implementation slices already recorded focused nextest/clippy evidence. Re-run the broader
workspace gate before a release or upstream PR.

## Follow-ons

Start a new workstream for any of these:

1. Standalone repository extraction for Jellyflow.
2. Public deprecation/removal of `fret-node` compatibility re-exports.
3. A future `jellyflow-geometry` crate, only after a real second consumer or cleaner pure-geometry
   seam appears.
4. Publishing or release automation for the Jellyflow crates.

## Residual Risks

- Compatibility re-exports are intentionally retained; removing them is a migration project, not a
  closeout cleanup.
- Geometry/spatial/hit-test helpers still live in `fret-node`; this is deliberate after JF-030, not
  unfinished extraction work.
- Standalone repository extraction remains deferred until package and publishing constraints are
  proven separately.
