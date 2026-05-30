# Jellyflow Standalone Readiness v1 - Evidence and Gates

Status: Active
Last updated: 2026-05-30

## Current Evidence Anchors

- `docs/workstreams/jellyflow-package-split-v1/CLOSEOUT_AUDIT_2026-05-30.md`
- `docs/workstreams/jellyflow-package-split-v1/JF-030_GEOMETRY_SPATIAL_AUDIT_2026-05-30.md`
- `docs/adr/0331-jellyflow-headless-node-graph-engine-boundary.md`
- `ecosystem/jellyflow-core/Cargo.toml`
- `ecosystem/jellyflow-core/src/lib.rs`
- `ecosystem/jellyflow-runtime/Cargo.toml`
- `ecosystem/jellyflow-runtime/src/lib.rs`
- `ecosystem/fret-node/Cargo.toml`

## Baseline Gates

- `cargo check -p jellyflow-core`
- `cargo check -p jellyflow-runtime`
- `cargo check -p fret-node --all-features --tests`
- `python3 tools/check_layering.py`
- `jq empty docs/workstreams/jellyflow-standalone-readiness-v1/WORKSTREAM.json`
- `python3 tools/check_workstream_catalog.py`
- `git diff --check`

## Gate Log

- 2026-05-30: Lane opened. Fresh gate evidence is pending the first execution slice.
