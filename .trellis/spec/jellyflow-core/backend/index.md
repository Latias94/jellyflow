# jellyflow-core Development Guidelines

`jellyflow-core` is the headless graph document and transaction crate. It must
remain portable and renderer-free.

## Source Anchors

- `crates/jellyflow-core/src/lib.rs`
- `crates/jellyflow-core/src/core/`
- `crates/jellyflow-core/src/ops/`
- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/adr/0002-jellyflow-model-policy-boundary.md`

## Pre-Development Checklist

- Read `.trellis/spec/guides/index.md`.
- Read this index plus the relevant files below.
- For model or persisted field changes, read ADR 0002 before editing.
- For public API changes, inspect crate-root re-exports in
  `crates/jellyflow-core/src/lib.rs`.
- For transaction changes, inspect existing `ops` helpers and tests before
  adding new mutation logic.

## Guidelines Index

| Guide | Use |
| --- | --- |
| [Directory Structure](./directory-structure.md) | Module ownership, file placement, naming, and examples. |
| [Error Handling](./error-handling.md) | Domain error enums, validation errors, and fallible operations. |
| [Quality Guidelines](./quality-guidelines.md) | Headless dependency guardrails, tests, and review checks. |

## Non-Negotiable Boundaries

- Do not add Fret, UI, renderer, platform, `wgpu`, `winit`, or egui
  dependencies.
- Keep `Graph` as the v1 persisted document shape unless an ADR-backed schema
  migration plan exists.
- Keep runtime store behavior, adapter callbacks, and renderer-specific behavior
  out of this crate.
- Prefer pure data and transaction contracts that higher layers can test without
  a renderer.
