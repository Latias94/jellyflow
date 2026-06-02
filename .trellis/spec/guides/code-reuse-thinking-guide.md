# Code Reuse Thinking Guide

Jellyflow already has many small pure helpers and focused test harnesses. Before
adding a new abstraction, first prove the existing contract is not enough.

## Search First

Use `rg` for names, concepts, and expected behavior. Good starting points:

- `crates/jellyflow-core/src/core/` for graph model, IDs, imports, symbols, and
  validation.
- `crates/jellyflow-core/src/ops/` for transactions, apply, diff, history,
  fragment, mutation, and normalization helpers.
- `crates/jellyflow-runtime/src/runtime/` for store, viewport, selection, drag,
  resize, rendering, geometry, lookups, conformance, and XyFlow compatibility.
- `crates/jellyflow-runtime/src/runtime/tests/` for reusable fixtures and harness
  helpers.
- `templates/headless-adapter/` before adding adapter-facing conformance behavior.

## Reuse Rules

- Reuse `jellyflow-core::ops` transaction and normalization helpers instead of
  building ad hoc graph mutations in runtime code.
- Reuse `runtime::policy` for effective interaction policy; do not duplicate
  override precedence in rules or adapters.
- Reuse `runtime::conformance` for adapter-feel contracts before adding
  renderer-specific smoke tests.
- Keep XyFlow-shaped vocabulary under `runtime::xyflow` or explicit conformance
  fixture APIs. Do not leak XyFlow names into canonical core model modules.
- Prefer existing test fixtures and harness helpers over new local builders when
  the scenario shape already exists.

## When A New Helper Is Justified

Add a helper only when it removes real repeated logic, clarifies a public
contract, or creates a stable adapter/conformance seam that existing callers can
share. Keep one-off test setup local to the test.

## Duplication Risks To Check

- Multiple modules deriving the same visible/render order or policy result.
- Local casts or parsing of persisted config that should live in `io/config`.
- New conformance actions that could be expressed through an existing action plus
  assertion.
- Public re-exports that duplicate an existing crate-root entry point.
- New geometry math that duplicates `runtime::geometry`, `runtime::fit_view`, or
  `runtime::utils`.

## Verification

After a reuse-sensitive change, run a focused search for the old and new concept
names. For Rust code, follow with the narrowest relevant `cargo nextest` gate and
expand to package/workspace checks only when the touched contract is shared.
