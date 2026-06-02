# Jellyflow Delete Contract v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

JDC-010 is complete: the lane is open and source coverage is recorded.

Jellyflow already has the core runtime behavior:

- `runtime::delete` exposes `plan_delete_selection`, key-bound planning, transaction builders, and
  store apply helpers.
- `runtime::keyboard` routes `KeyboardIntent::DeleteSelection` and
  `KeyboardIntent::DeleteSelectionForKey` to the delete helpers.
- conformance actions already expose `apply_delete_selection` and `apply_delete_selection_for_key`.
- focused runtime tests already cover node deletion, edge deletion, key gates, policy rejection,
  empty selection no-op, and stale view-state cleanup.

The first gap is template coverage: `templates/headless-adapter` currently demonstrates drag,
resize, viewport, viewport animation, and pan inertia, but not delete selection.

## Next Task

JDC-020: add a template adapter delete selection scenario that uses
`ConformanceAction::apply_delete_selection_for_key`, expects the delete commit and XyFlow-style
callbacks, and proves selection cleanup in the trace.

## Decisions Since Opening

- Treat delete planner ownership as mostly implemented behavior that needs contract promotion, not
  a from-scratch planner task.
- Keep async `onBeforeDelete` parity as a follow-on. Adapters may run confirmation or async policy
  before calling Jellyflow.
- Keep DOM key handling outside runtime. Runtime accepts normalized `KeyboardIntent` or direct
  delete-selection calls.

## Validation To Run

For JDC-020:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

For closeout:

```bash
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-delete-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-delete-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-delete-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-delete-contract-v1/CONTEXT.jsonl
git diff --check
```

## Next Recommended Action

Start JDC-020 in `templates/headless-adapter/src/lib.rs`. Add a delete scenario with one selected
node and its connected edge, then expect graph commit, node/edge changes, delete callbacks, and
selection cleanup trace events.
