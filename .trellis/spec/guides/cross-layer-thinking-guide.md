# Cross-Layer Thinking Guide

Jellyflow's main risk is boundary drift: core model, runtime behavior,
conformance fixtures, XyFlow compatibility, and future adapters must stay
separate while still describing one editor feel.

## Boundary Map

- `jellyflow-core` owns the persisted graph document model, stable IDs, type
  descriptors, interaction value types, and undoable graph transactions.
- `jellyflow-runtime` owns `NodeGraphStore`, view/config payloads, rules,
  schema/profile hooks, policy resolution, headless interaction helpers,
  geometry, rendering helpers, and conformance fixtures.
- `runtime::xyflow` is the explicit compatibility vocabulary for XyFlow-shaped
  changes, callbacks, and controlled-mode projections.
- `templates/headless-adapter` proves external consumers can run headless
  conformance checks without Fret or renderer dependencies.
- Renderer, platform, screenshot, and pixel behavior belongs in future adapter
  crates, not in `jellyflow-core` or `jellyflow-runtime`.

## Before Changing A Cross-Layer Contract

Check the relevant ADRs:

- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/adr/0002-jellyflow-model-policy-boundary.md`
- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`

Then map every affected layer:

- source module and crate-root re-exports;
- serde shape or persisted file type;
- runtime store method or helper;
- XyFlow projection/callback contract;
- conformance action, runner, fixture, and approval path;
- headless adapter template scenario;
- README, `CONTEXT.md`, and workstream evidence.

## Common Mistakes

- Adding Fret, renderer, `wgpu`, `winit`, or egui dependencies to headless crates.
- Moving persisted policy/layout fields out of `Graph` without an ADR-backed
  migration plan.
- Adding an adapter-facing behavior without conformance coverage.
- Adding a renderer smoke test as the first proof of headless behavior.
- Treating a closed workstream as an active task instead of historical evidence.

## Legacy Workstream Migration

Closed workstreams under `docs/workstreams/` remain evidence. A new Trellis task
should cite the relevant old `DESIGN.md`, `EVIDENCE_AND_GATES.md`, `HANDOFF.md`,
and `WORKSTREAM.json` in its planning artifacts when prior work constrains the
new scope. Do not copy all closed workstreams into `.trellis/tasks/`.

## Verification

Use focused gates first, then expand:

- `cargo fmt --check`
- `cargo nextest run -p jellyflow-core <filter>`
- `cargo nextest run -p jellyflow-runtime <filter>`
- `cargo nextest run --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `python3 tools/check_no_fret_dependencies.py`
- `python3 tools/check_external_consumer_smoke.py`
- `git diff --check`
