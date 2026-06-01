# Jellyflow Node Drag Kernel v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-01

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime adapter_conformance
```

This currently exercises the interaction harness and connect gesture contract. JND-020 should add a
drag-focused test filter.

## Gate Set

### Drag Kernel Gate

```bash
cargo nextest run -p jellyflow-runtime drag
cargo check -p jellyflow-runtime
```

This proves the headless drag kernel behavior without renderer dependencies.

### Adapter Conformance Gate

```bash
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo check -p jellyflow-runtime
```

This proves drag behavior integrates with the runtime harness and XyFlow compatibility projection.

### Runtime Package Gate

```bash
cargo nextest run -p jellyflow-runtime
```

This proves drag changes do not regress runtime behavior.

### Broader Closeout Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
jq empty docs/workstreams/jellyflow-node-drag-kernel-v1/WORKSTREAM.json
git diff --check
```

This proves formatting, runtime behavior, lint cleanliness, JSON validity, and diff hygiene.

## Evidence Anchors

- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `docs/adr/0002-jellyflow-model-policy-boundary.md`
- `docs/workstreams/jellyflow-interaction-harness-v1/HANDOFF.md`
- `docs/workstreams/jellyflow-geometry-spatial-v1/HANDOFF.md`
- `repo-ref/xyflow/packages/system/src/xydrag/XYDrag.ts`
- `repo-ref/xyflow/packages/system/src/xydrag/utils.ts`
- `crates/jellyflow-core/src/core/model/node.rs`
- `crates/jellyflow-core/src/ops/transaction/op.rs`
- `crates/jellyflow-runtime/src/runtime/policy/node.rs`

## Fresh Evidence Log

- 2026-06-01: JND-010 opened the node drag kernel workstream.
  - `git status --short --branch`: clean before opening docs, branch ahead of origin.
  - Governing decisions: ADR 0003 keeps drag fixtures renderer-free; ADR 0002 keeps persisted
    layout/policy fields in `Graph` for v1.
  - XyFlow reference reviewed: `xydrag/XYDrag.ts` and `xydrag/utils.ts`.
- 2026-06-01: JND-020 added the first single-node drag kernel slice.
  - Added public `runtime::drag` with `NodeDragRequest`, `NodeDragPlan`,
    `NODE_DRAG_TRANSACTION_LABEL`, `plan_node_drag`, and `NodeGraphStore::apply_node_drag`.
  - Added harness-backed drag fixtures for a committed `SetNodePos` trace and no-commit behavior.
  - Fixture coverage: per-node draggable policy, global `nodes_draggable`, hidden-node exclusion,
    missing node, no-op target, non-finite target, deterministic transaction label/op, and
    committed graph state.
  - `review-workstream` self-review: no blocking findings; continuous drag preview/final-commit
    semantics remain a named follow-on risk.
  - RED gate: `cargo nextest run -p jellyflow-runtime drag` failed before `runtime::drag` and
    `NodeGraphStore::apply_node_drag` existed.
  - `cargo fmt`: applied formatting after `cargo fmt --check` reported style-only differences.
  - `cargo fmt --check`: passed after formatting.
  - `cargo nextest run -p jellyflow-runtime drag`: passed, 4 tests.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 2 tests.
  - `cargo check -p jellyflow-runtime`: passed.
  - `cargo nextest run -p jellyflow-runtime`: passed, 145 tests.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.

## Notes

- Fresh verification is required before marking a task, Codex goal, or lane complete.
- Keep renderer input capture, screenshots, and pixel tests outside `jellyflow-runtime`.
- Do not treat this lane as permission to move persisted node layout or policy fields out of
  `Graph`.
