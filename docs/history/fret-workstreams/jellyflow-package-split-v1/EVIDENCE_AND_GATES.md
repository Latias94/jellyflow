# Jellyflow Package Split v1 - Evidence And Gates

Status: Closed
Last updated: 2026-05-30

## Current Gates

```bash
cargo check -p jellyflow-core
cargo nextest run -p jellyflow-core
cargo clippy -p jellyflow-core --all-targets -- -D warnings
cargo check -p jellyflow-runtime
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
cargo check -p fret-node --all-features --tests
cargo nextest run -p fret-node --no-default-features
cargo fmt --check
python3 tools/check_layering.py
jq empty docs/workstreams/jellyflow-package-split-v1/WORKSTREAM.json
python3 tools/check_workstream_catalog.py
git diff --check
```

## Fresh Evidence

- `cargo search jellyflow --limit 5`: returned no matches during the local check on 2026-05-29.
  This is a convenience check only, not a publishing guarantee.
- `cargo check -p jellyflow-core`: passed after creating the crate.
- `cargo check -p fret-node --all-features --tests`: passed after adding compatibility wrapper
  modules.
- `cargo nextest run -p jellyflow-core`: passed with 14 tests.
- `cargo clippy -p jellyflow-core --all-targets -- -D warnings`: passed.
- `cargo nextest run -p fret-node --no-default-features`: passed with 124 tests.
- `cargo fmt --check`: passed.
- `jq empty docs/workstreams/jellyflow-package-split-v1/WORKSTREAM.json`: passed.
- `git diff --check`: passed.
- `python3 tools/check_layering.py`: passed.
- `cargo check -p fret-node --all-features --tests`: passed after moving `ops` into
  `jellyflow-core` and adapting the adapter-side change projection.
- `cargo nextest run -p fret-node --no-default-features`: passed with 90 tests after the `ops`
  split.
- `cargo fmt --check`: passed after rustfmt cleanup.
- `python3 tools/check_layering.py`: passed after the `ops` split.
- `cargo check -p jellyflow-runtime`: passed after creating the runtime crate.
- `cargo nextest run -p jellyflow-runtime`: passed with 67 tests.
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
- `cargo check -p fret-node --all-features --tests`: passed after adding runtime compatibility
  wrappers.
- `cargo nextest run -p fret-node --no-default-features`: passed with 24 tests after the runtime
  split.
- `cargo fmt --check`: passed after the runtime split.
- `python3 tools/check_layering.py`: passed after the runtime split.
- `git diff --check`: passed after the runtime split.
- `jq empty docs/workstreams/jellyflow-package-split-v1/WORKSTREAM.json`: passed after the runtime
  split.
- `python3 tools/audit_crate.py --crate fret-node`: produced the geometry/spatial audit snapshot
  used for the JF-030 decision.
- `docs/workstreams/jellyflow-package-split-v1/JF-030_GEOMETRY_SPATIAL_AUDIT_2026-05-30.md`:
  recorded the decision to keep geometry/spatial in `fret-node` for now.
- `docs/workstreams/jellyflow-package-split-v1/CLOSEOUT_AUDIT_2026-05-30.md`: records the
  closeout state and follow-on split policy.
- `cargo check -p jellyflow-runtime`: passed during closeout.
- `cargo check -p fret-node --all-features --tests`: passed during closeout.
- `python3 tools/check_layering.py`: passed during closeout.
- `jq empty docs/workstreams/jellyflow-package-split-v1/WORKSTREAM.json`: passed during closeout.
- `python3 tools/check_workstream_catalog.py`: passed during closeout, validating 510 dedicated
  directories and 47 standalone markdown files.
- `git diff --check`: passed during closeout.

## Evidence Anchors

- `docs/adr/0331-jellyflow-headless-node-graph-engine-boundary.md`
- `ecosystem/jellyflow-core/Cargo.toml`
- `ecosystem/jellyflow-core/src/lib.rs`
- `ecosystem/jellyflow-core/src/ops/mod.rs`
- `ecosystem/jellyflow-core/src/ops/{apply,build,diff,fragment,history,normalize,tests,tx_sanity}.rs`
- `ecosystem/jellyflow-runtime/Cargo.toml`
- `ecosystem/jellyflow-runtime/src/lib.rs`
- `ecosystem/jellyflow-runtime/src/{io,profile,rules,schema,runtime}/`
- `ecosystem/fret-node/Cargo.toml`
- `ecosystem/fret-node/src/core/mod.rs`
- `ecosystem/fret-node/src/types/mod.rs`
- `ecosystem/fret-node/src/interaction/mod.rs`
- `ecosystem/fret-node/src/ops/mod.rs`
- `ecosystem/fret-node/src/{io,profile,rules,schema,runtime}/mod.rs`
- `docs/workstreams/jellyflow-package-split-v1/JF-030_GEOMETRY_SPATIAL_AUDIT_2026-05-30.md`
- `docs/workstreams/jellyflow-package-split-v1/CLOSEOUT_AUDIT_2026-05-30.md`
