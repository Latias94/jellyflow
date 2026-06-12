# Changelog

All notable changes to this project will be documented in this file.

The format is based on *Keep a Changelog*, and this project adheres to *Semantic Versioning*.

## [Unreleased]

No unreleased changes yet.

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
