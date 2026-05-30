# JSR-020 External Headless Consumer Smoke

Date: 2026-05-30
Status: Complete

## Summary

JSR-020 adds a repeatable external-consumer smoke gate for Jellyflow. The gate creates a temporary
Cargo project outside the Fret workspace, path-depends only on `jellyflow-core` and
`jellyflow-runtime`, runs `cargo check`, then inspects `cargo tree` and fails if any `fret` or
`fret-*` package is pulled transitively.

The smoke project intentionally does not depend on `fret-node` or `fret-core`.

## Fixture

Repeatable gate:

```bash
python3 tools/check_jellyflow_external_smoke.py
```

The script writes a temporary package named `jellyflow-external-smoke` with this dependency shape:

```toml
[dependencies]
jellyflow-core = { path = "<repo>/ecosystem/jellyflow-core" }
jellyflow-runtime = { path = "<repo>/ecosystem/jellyflow-runtime" }
```

The Rust smoke exercises the smallest useful headless flow:

- construct a `Graph` and `Node`,
- apply `GraphTransaction::AddNode`,
- create `NodeGraphStore`,
- dispatch `GraphTransaction::SetNodePos`,
- compute a fit-view target from a `CanvasRect`,
- verify Jellyflow-owned `NodeGraphModifiers`.

## Contract Proven

- External path-dependency consumers can compile against `jellyflow-core` and `jellyflow-runtime`
  without depending on `fret-node`.
- The remaining Fret adapter boundary is not pulled transitively by the headless runtime.
- `fret-core` is no longer required for the public input/geometry contracts used by the smoke.
- The smoke gate is source-controlled and can be reused during JSR-030 repository-policy work.

## Limits

This is not a publish-readiness proof. Because the source crates still live inside the Fret
monorepo, publish metadata, README/API docs, license packaging, release-plz integration, and
history-preserving repository extraction remain JSR-030 concerns.

## Fresh Gates

- `python3 tools/check_jellyflow_external_smoke.py`: passed.
  - `cargo check --manifest-path <temp>/Cargo.toml`: passed.
  - `cargo tree --manifest-path <temp>/Cargo.toml --prefix none`: passed; no `fret` or `fret-*`
    packages were present.
- `python3 -m py_compile tools/check_jellyflow_external_smoke.py`: passed.
- `cargo check -p jellyflow-core`: passed.
- `cargo check -p jellyflow-runtime`: passed.
- `cargo check -p fret-node --all-features --tests`: passed.
- `python3 tools/check_layering.py`: passed.
- `jq empty docs/workstreams/jellyflow-standalone-readiness-v1/WORKSTREAM.json`: passed.
- `python3 tools/check_workstream_catalog.py`: passed.
- `git diff --check`: passed.
- `cargo fmt --check`: passed.
