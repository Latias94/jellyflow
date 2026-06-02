# Repository Development Guidelines

Repository-level work covers CI, release documentation, workspace metadata,
Trellis configuration, and cross-crate validation. Keep this layer focused on
repository contracts, not runtime behavior.

## Source Anchors

- `Cargo.toml`
- `Cargo.lock`
- `.github/workflows/`
- `docs/releasing/`
- `README.md`
- `.trellis/config.yaml`
- `.trellis/spec/`

## Pre-Development Checklist

- Read `.trellis/spec/guides/index.md`.
- Read this index plus [Release Readiness](./release-readiness.md) before
  changing CI, package metadata, release docs, or publish workflow files.
- Read package specs for any crate whose manifest or public API changes.

## Guidelines Index

| Guide | Use |
| --- | --- |
| [Release Readiness](./release-readiness.md) | CI gates, Cargo packaging, dry-run, and publish-order contracts. |

## Quality Check

- Confirm repository-level changes do not add Fret, UI, renderer, platform,
  `wgpu`, `winit`, or egui dependencies to headless crates.
- Prefer exact Rust toolchains in CI when enforcing `rust-version`.
- Run the release-readiness gates relevant to the touched files.
- Keep actual crates.io publishing and release automation out of scope unless
  the active task explicitly includes them.
