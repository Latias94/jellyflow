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

## Notes

- Fresh verification is required before marking a task, Codex goal, or lane complete.
- Keep renderer input capture, screenshots, and pixel tests outside `jellyflow-runtime`.
- Do not treat this lane as permission to move persisted node layout or policy fields out of
  `Graph`.
