# Changelog

All notable changes to this project will be documented in this file.

The format is based on *Keep a Changelog*, and this project adheres to *Semantic Versioning*.

## [Unreleased]

### Changed

- Changed `Graph` collection accessors to return read-only `GraphElements` views instead of direct
  `BTreeMap` references, keeping public graph reads stable while leaving internal storage free to
  evolve.
- Changed `NodeGraphPatch` to expose committed data through `transaction()`, `footprint()`, and
  `into_parts()` accessors instead of public fields, keeping cached mutation footprints consistent
  with their transactions.
- Changed `DispatchOutcome` to expose its patch through `patch()` and `into_patch()` instead of a
  public field, keeping dispatch payloads on the same accessor-based invalidation contract.

### Added

- Added `GraphMutationFootprint` and transaction footprint helpers so hosts can derive invalidation,
  collaboration, and indexing boundaries from normal `GraphOp` / `GraphTransaction` values.
- Added layout dirty-scope helpers that derive `LayoutScope::Nodes` from a transaction or mutation
  footprint using the current graph snapshot.
- Added cached mutation footprints on runtime `NodeGraphPatch` / `DispatchOutcome` values so
  middleware, subscribers, and controlled integrations can consume touched ids directly.
- Added `NodeSchemaBuilder` and `PortDecl` builder helpers for renderer-neutral custom node
  catalogs, including renderer keys, default data, default size, categories, keywords, aliases, and
  declared ports.
- Added built-in layout metadata consistency coverage so preset builders, registry entries, engine
  metadata, family metadata, capabilities, and default options stay aligned.
- Added runtime layout-pipeline benchmark smoke coverage for footprint-derived dirty scopes.
- Added the `jellyflow-egui` immediate-mode adapter crate with a demo app, palette, toolbar,
  inspector, canvas rendering, and pointer input.
- Added `jellyflow-egui` canvas interactions for toolbar modes, handle-to-handle connections,
  selected-node resize handles, edge selection, pan mode, drag previews, and connection/resize
  feedback.
- Added `jellyflow-egui` keyboard nudging and cursor feedback for editor-grade canvas interactions.
- Added release, CI package-list, dependency-boundary, and external-consumer smoke coverage for
  `jellyflow-egui`.
- Added public-surface and external-consumer smoke coverage for read-only graph views, dispatch
  patch accessors, custom node schema builders, built-in layout presets, and dirty-scope helpers.

## [0.2.0] - 2026-06-13

### Added

- Added the built-in `tidy_tree` layout engine and made `LayoutPresetBuilder::tree()` target it by
  default.
- Added Criterion benchmarks for built-in layout engines.
- Added `LayoutPresetBuilder` for workflow, tree, radial mind-map, and freeform mind-map layout
  request presets.

### Changed

- Consolidated repeated native layout projection and result-building helpers.

## [0.1.0] - 2026-06-12

Initial public release line for Jellyflow. This release establishes the headless graph engine,
adapter boundary, layout extension point, release documentation, and crates.io automation.

### Added

- Added the top-level `jellyflow` facade crate. It re-exports `jellyflow-core`,
  `jellyflow-layout`, and `jellyflow-runtime` under `core`, `layout`, and `runtime`, and provides a
  small prelude for common graph-store setup.
- Added release-facing README guidance modeled after a published Rust workspace: entry-point
  selection, install commands, quickstarts, adapter conformance, performance, quality gates, and
  workspace crate roles.
- Added release CI workflows for manual crates.io preflight checks, dependency-ordered publishing,
  and GitHub Release creation.
- Added external consumer smoke coverage for applications that depend on the top-level
  `jellyflow` facade crate.
- Added runtime benchmark smoke coverage for `rendering_query` and `schema_create_node`.

### Changed

- Updated crates.io publishing documentation for the four-crate release order:
  `jellyflow-core`, `jellyflow-layout`, `jellyflow-runtime`, then `jellyflow`.
- Expanded dependency-boundary checks so all publishable crates reject accidental `fret` or
  `fret-*` dependencies.
- Committed the workspace `Cargo.lock` and aligned CI/tooling with Rust `1.95`.
- Updated the headless adapter template to use the workspace MSRV.

### Fixed

- Fixed CI locked-check failures by committing the workspace lockfile and keeping the root
  `Cargo.lock` unignored.
