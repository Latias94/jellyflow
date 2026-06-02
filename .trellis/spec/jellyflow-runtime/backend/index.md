# jellyflow-runtime Development Guidelines

`jellyflow-runtime` is the headless store, rules, schema/profile, interaction,
geometry, rendering, XyFlow-compatibility, and conformance crate built on
`jellyflow-core`.

## Source Anchors

- `crates/jellyflow-runtime/src/lib.rs`
- `crates/jellyflow-runtime/src/runtime/`
- `crates/jellyflow-runtime/src/io/`
- `crates/jellyflow-runtime/src/rules/`
- `crates/jellyflow-runtime/src/schema/`
- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/adr/0002-jellyflow-model-policy-boundary.md`
- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`

## Pre-Development Checklist

- Read `.trellis/spec/guides/index.md`.
- Read this index plus the relevant files below.
- For renderer/adapter/testing questions, read ADR 0003.
- For policy behavior, inspect `runtime::policy` before changing rules.
- For adapter-facing behavior, inspect `runtime::conformance` and
  `runtime::xyflow` before adding a new public API.
- For public API changes, inspect crate-root re-exports and
  `crates/jellyflow-runtime/tests/public_surface.rs`.

## Guidelines Index

| Guide | Use |
| --- | --- |
| [Directory Structure](./directory-structure.md) | Runtime module ownership, file placement, and examples. |
| [Error Handling](./error-handling.md) | Store/rules/io/conformance error patterns. |
| [Quality Guidelines](./quality-guidelines.md) | Headless dependency guardrails, conformance gates, and review checks. |

## Non-Negotiable Boundaries

- Do not add Fret, UI, renderer, platform, `wgpu`, `winit`, or egui
  dependencies.
- Test XyFlow-like editor feel through headless runtime/conformance contracts
  before renderer smoke tests.
- Keep XyFlow-shaped names under `runtime::xyflow` or explicit conformance APIs.
- Keep renderer smoke harnesses in future adapter crates, not in runtime.
