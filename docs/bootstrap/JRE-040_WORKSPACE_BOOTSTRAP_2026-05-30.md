# JRE-040 Workspace Bootstrap

Date: 2026-05-30
Status: Complete

## Scope

This bootstrap turns the filtered Jellyflow repository into a standalone Rust workspace.

## Changes

- Added root workspace metadata in `Cargo.toml`.
- Added root `README.md`, `.gitignore`, `LICENSE-MIT`, and `LICENSE-APACHE`.
- Added package READMEs for `jellyflow-core` and `jellyflow-runtime`.
- Added minimal examples:
  - `crates/jellyflow-core/examples/build_graph.rs`
  - `crates/jellyflow-runtime/examples/store_dispatch.rs`
- Removed filtered `ecosystem/fret-node/src/*/mod.rs` compatibility wrapper remnants.
- Updated `tools/check_external_consumer_smoke.py` to consume `crates/jellyflow-*` paths.

## Validation

- `cargo check --workspace`: passed.
- `cargo nextest run --workspace`: passed with 115 tests.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --check`: passed.
- `cargo run -p jellyflow-core --example build_graph`: passed.
- `cargo run -p jellyflow-runtime --example store_dispatch`: passed.
- `python3 tools/check_external_consumer_smoke.py`: passed; external cargo tree contained no Fret
  packages.
- `git diff --check`: passed.
