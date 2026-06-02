# Jellyflow Delete Contract v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

JDC-010 is complete: the lane is open and source coverage is recorded.

JDC-020 is complete: the headless adapter template now includes a delete selection scenario using
`apply_delete_selection_for_key(Backspace)`, a single-scenario smoke helper, 7-scenario suite
assertions, and README coverage.

Jellyflow already has the core runtime behavior:

- `runtime::delete` exposes `plan_delete_selection`, key-bound planning, transaction builders, and
  store apply helpers.
- `runtime::keyboard` routes `KeyboardIntent::DeleteSelection` and
  `KeyboardIntent::DeleteSelectionForKey` to the delete helpers.
- conformance actions already expose `apply_delete_selection` and `apply_delete_selection_for_key`.
- focused runtime tests already cover node deletion, edge deletion, key gates, policy rejection,
  empty selection no-op, and stale view-state cleanup.

The remaining gap is documentation/closeout: root/runtime docs should present delete as a
first-class runtime interaction contract, and stale follow-on navigation should be cleared.

## Next Task

JDC-030: document delete runtime/adapter boundaries, record closeout evidence, and close or split
follow-ons.

## Decisions Since Opening

- Treat delete planner ownership as mostly implemented behavior that needs contract promotion, not
  a from-scratch planner task.
- Keep async `onBeforeDelete` parity as a follow-on. Adapters may run confirmation or async policy
  before calling Jellyflow.
- Keep DOM key handling outside runtime. Runtime accepts normalized `KeyboardIntent` or direct
  delete-selection calls.
- JDC-020 added explicit `keyboard-types` to the external template because the template now models
  adapter-owned key-code normalization directly.

## Validation To Run

JDC-020 passed:

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

For JDC-030 closeout:

```bash
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-delete-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-delete-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-delete-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-delete-contract-v1/CONTEXT.jsonl
git diff --check
```

## Next Recommended Action

Start JDC-030 by updating README/runtime README and closeout docs:

- explain runtime owns selection delete planning and deterministic transactions;
- explain adapters own platform key capture, focus/input suppression, confirmation dialogs, and
  async pre-delete hooks;
- keep renderer smoke and pixel checks in adapter crates.
