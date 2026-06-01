# JSR-010 Standalone Extraction Inventory

Status: Complete
Last updated: 2026-05-30

## Verdict

Jellyflow is ready for a standalone-repository planning slice, but it is not ready for a physical
repository extraction or crates.io publish yet.

The current headless crates already avoid Fret UI, renderer, platform, runner, `wgpu`, and `winit`
dependencies. The main blocker is narrower: both Jellyflow crates still depend on `fret-core` for a
small input/geometry vocabulary. A standalone Jellyflow repository should not require consumers to
pull Fret core unless that is an explicit transitional policy.

User preference recorded during this slice: the target local repository path is
`~/codes/rust/jellyflow`, and JSR-030 should assume a new independent repository with
history-preserving extraction as the default policy.

## Package Inventory

| Package | Role | Direct workspace deps | Direct external deps | Publish metadata state |
| --- | --- | --- | --- | --- |
| `jellyflow-core` | Headless graph document model, IDs, type descriptors, interaction value types, transaction ops/history | `fret-core` | `serde`, `serde_json`, `thiserror`, `uuid` | Has description/license via workspace, no package README, repository/homepage/docs still point at Fret |
| `jellyflow-runtime` | Headless I/O/view-state payloads, rules, schema/profile pipeline, store/callbacks/fit-view | `jellyflow-core`, `fret-core` | `serde`, `serde_json`, `thiserror`, `uuid` | Has description/license via workspace, no package README, repository/homepage/docs still point at Fret |
| `fret-node` | Fret adapter and compatibility facade over Jellyflow-backed node graph surfaces | `jellyflow-core`, `jellyflow-runtime`, `fret-core`; optional `fret-ui`, `fret-runtime`, `fret-canvas`, `fret-ui-kit`, `fret-app` | `serde`, `serde_json`, `slotmap`, `thiserror`, `tracing`, `uuid` | Has README and Fret metadata; should stay in the Fret repo |

## Boundary Findings

### Headless Crates Avoid UI/Renderer Leakage

Evidence:

- `cargo tree -p jellyflow-core --depth 2`
- `cargo tree -p jellyflow-runtime --depth 2`
- `cargo tree -p fret-node --no-default-features --features headless --depth 2`
- Manifest source-policy tests in `ecosystem/jellyflow-core/src/lib.rs` and
  `ecosystem/jellyflow-runtime/src/lib.rs`

The Jellyflow crates do not directly depend on `fret-ui`, `fret-runtime`, `fret-canvas`, `wgpu`, or
`winit`. `fret-node` still pulls those UI/adapter dependencies behind its default `fret-ui` feature,
which is correct because `fret-node` remains the Fret adapter.

### `fret-core` Is The Standalone Blocker

Current `fret-core` usage is small but public enough that it should be handled before the new repo
is treated as standalone:

- `ecosystem/jellyflow-core/src/interaction/mod.rs` uses `fret_core::Modifiers`.
- `ecosystem/jellyflow-runtime/src/io/mod.rs` exposes `NodeGraphKeyCode(pub fret_core::KeyCode)`.
- `ecosystem/jellyflow-runtime/src/runtime/fit_view.rs` exposes
  `compute_fit_view_target_for_canvas_rect(target_canvas: fret_core::Rect, ...)`.
- Runtime fit-view tests also use `fret_core::{Point, Px, Rect, Size}`.

Recommended extraction requirement:

1. Replace `Modifiers` with a Jellyflow-owned modifier value type or an external input crate type.
2. Replace `fret_core::KeyCode` with a Jellyflow-owned wrapper over `keyboard_types::Code`, or make
   key matching string-first so serialized config is the stable contract.
3. Replace the public `fret_core::Rect` fit-view API with `CanvasRect` or a Jellyflow-owned rect
   type. Fret adapter conversions can live in `fret-node`.
4. Keep an explicit compatibility bridge in `fret-node` when Fret UI still wants Fret geometry or
   keyboard types.

### Workspace Metadata Must Become Jellyflow Metadata

Both Jellyflow crates inherit workspace fields:

- `version.workspace = true`
- `edition.workspace = true`
- `rust-version.workspace = true`
- `authors.workspace = true`
- `license.workspace = true`
- `repository.workspace = true`
- `homepage.workspace = true`
- `documentation.workspace = true`

This is fine in Fret, but the new repo needs package-local or Jellyflow-workspace values:

- repository/homepage should point to the Jellyflow repo, not `https://github.com/Latias94/fret`;
- documentation should point to Jellyflow docs or package docs, not `https://docs.rs/fret`;
- `jellyflow-core` and `jellyflow-runtime` need package READMEs;
- keywords/categories should be considered before crates.io publish;
- the inherited Fret MSRV `1.92` may be too high for a headless graph engine and should be
  decided independently of Fret's GPU stack;
- workspace dependency versions need to be owned by the Jellyflow repo, not by Fret's root
  `Cargo.toml`.

`cargo package -p jellyflow-core --list --allow-dirty` and
`cargo package -p jellyflow-runtime --list --allow-dirty` both succeeded as inventory commands, but
they do not prove publish readiness.

## Public Surface And Compatibility

`jellyflow-core` exports:

- modules: `core`, `interaction`, `ops`, `types`
- root re-exports for graph IDs/model types, interaction value types, transaction/history helpers,
  and type compatibility primitives

`jellyflow-runtime` exports:

- compatibility-style module re-exports for `core`, `interaction`, `ops`, and `types`
- modules: `io`, `profile`, `rules`, `runtime`, `schema`
- root re-exports for `GraphProfile`, apply helpers, `DispatchError`, `DispatchOutcome`, and
  `NodeGraphStore`

`fret-node` keeps existing compatibility paths:

- `fret_node::{core,types,interaction,ops}` re-export `jellyflow-core`
- `fret_node::{io,profile,rules,schema,runtime}` re-export `jellyflow-runtime`
- Fret UI surfaces stay behind `#[cfg(feature = "fret-ui")]`
- kit profiles such as `DataflowProfile` stay in `fret-node`

The compatibility facade should remain in Fret while the standalone Jellyflow repo owns only the
headless crates. Public deprecation or removal of `fret_node::*` compatibility paths is a separate
lane.

## History Extraction Inputs

The new repository should preserve Jellyflow-relevant history through a path-filtered rewrite, not a
snapshot copy.

Observed history anchors:

- `0ee9543b67 feat(jellyflow): extract headless core from fret-node`
- `8eb1cc90fd refactor(jellyflow): move ops into jellyflow-core`
- `fa775ecb92 refactor(jellyflow): extract headless runtime crate`
- `035b329937 feat(node): scaffold fret-node crate`

Initial path set for JSR-030:

- current paths:
  - `ecosystem/jellyflow-core/`
  - `ecosystem/jellyflow-runtime/`
- historical source paths to preserve pre-split history:
  - `ecosystem/fret-node/src/core/`
  - `ecosystem/fret-node/src/types/`
  - `ecosystem/fret-node/src/interaction/`
  - `ecosystem/fret-node/src/ops/`
  - `ecosystem/fret-node/src/io/`
  - `ecosystem/fret-node/src/profile/`
  - `ecosystem/fret-node/src/rules/`
  - `ecosystem/fret-node/src/schema/`
  - `ecosystem/fret-node/src/runtime/`
- contract docs to consider carrying or copying:
  - `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
  - `docs/workstreams/jellyflow-package-split-v1/`
  - `docs/workstreams/jellyflow-standalone-readiness-v1/`

Recommended JSR-030 toolchain:

- use a fresh clone of Fret, not the working repo;
- run `git filter-repo --analyze` before rewriting;
- use `--paths-from-file` for the source path set;
- use `--path-rename` only after the old-to-new crate layout map is explicit;
- record the source repo, source commit, filter script, and path map in the new repo;
- validate with `git log --follow`, `git blame`, and focused `cargo check`.

Commit SHAs will change after history rewrite. The preservation goal is authorship, timestamps,
commit messages, filtered parent structure, and per-file evolution history.

## Size And Maintainability Notes

Audit snapshots found large test and runtime files:

- `ecosystem/jellyflow-core/src/ops/tests.rs`: 1568 lines
- `ecosystem/jellyflow-runtime/src/runtime/tests.rs`: 2018 lines
- `ecosystem/jellyflow-runtime/src/io/mod.rs`: 1633 lines
- `ecosystem/jellyflow-runtime/src/runtime/store.rs`: 906 lines
- `ecosystem/jellyflow-runtime/src/rules/mod.rs`: 849 lines
- `ecosystem/jellyflow-runtime/src/runtime/utils.rs`: 802 lines

These are not blockers for repository extraction, but they should become early standalone cleanup
candidates once the boundary and external smoke are green. The external repo should avoid carrying
Fret's broad lint allowlist indefinitely.

## Recommended Next Slice

Add JSR-015 before the external smoke:

1. Remove or consciously replace the `fret-core` dependency from `jellyflow-core` and
   `jellyflow-runtime`.
2. Keep any Fret type conversions in `fret-node`.
3. Re-run focused checks and no-default `fret-node` compatibility gates.
4. Then run JSR-020 as a true external consumer smoke that does not need Fret crates.

## Fresh Gate Evidence

- `python3 tools/audit_crate.py --crate jellyflow-core`: passed; recorded exports, deps, and file
  size snapshot.
- `python3 tools/audit_crate.py --crate jellyflow-runtime`: passed; recorded exports, deps, and
  file size snapshot.
- `python3 tools/audit_crate.py --crate fret-node`: passed; recorded adapter deps, exports, and
  large UI test files.
- `cargo tree -p jellyflow-core --depth 2`: passed; shows `fret-core` plus external deps only.
- `cargo tree -p jellyflow-runtime --depth 2`: passed; shows `jellyflow-core`, `fret-core`, and
  external deps only.
- `cargo tree -p fret-node --no-default-features --features headless --depth 2`: passed; shows no
  UI/render/platform deps in the explicit headless build.
- `cargo check -p jellyflow-core`: passed.
- `cargo check -p jellyflow-runtime`: passed.
- `cargo check -p fret-node --all-features --tests`: passed.
- `python3 tools/check_layering.py`: passed.
- `python3 tools/report_largest_files.py --top 30 --min-lines 800`: passed; `fret-node` UI tests
  remain visible as a large adapter-side file.
