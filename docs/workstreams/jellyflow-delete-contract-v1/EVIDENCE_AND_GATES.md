# Jellyflow Delete Contract v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-02

## Smallest Current Repro

```bash
cargo nextest run -p jellyflow-runtime conformance
```

JDC-020 should keep delete selection coverage passing and add template smoke coverage.

## Gate Set

### Template Delete Smoke Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

### Package And Closeout Gate

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
```

### Metadata And Diff Gate

```bash
jq empty docs/workstreams/jellyflow-delete-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-delete-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-delete-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-delete-contract-v1/CONTEXT.jsonl
git diff --check
```

## Evidence Anchors

- `docs/workstreams/jellyflow-delete-contract-v1/DESIGN.md`
- `docs/workstreams/jellyflow-delete-contract-v1/TODO.md`
- `docs/workstreams/jellyflow-delete-contract-v1/TASKS.jsonl`
- `docs/workstreams/jellyflow-delete-contract-v1/CAMPAIGNS.jsonl`
- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/adr/0002-jellyflow-model-policy-boundary.md`
- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `docs/workstreams/jellyflow-model-policy-boundary-v1/HANDOFF.md`
- `repo-ref/xyflow/packages/system/src/utils/graph.ts`
- `repo-ref/xyflow/packages/react/src/hooks/useReactFlow.ts`
- `repo-ref/xyflow/packages/react/src/hooks/useGlobalKeyHandler.ts`
- `crates/jellyflow-runtime/src/runtime/delete/`
- `crates/jellyflow-runtime/src/runtime/keyboard/`
- `crates/jellyflow-runtime/src/runtime/conformance/`

## Evidence Log

### 2026-06-02 - JDC-010 Workstream Opened

Scope: `docs/workstreams/jellyflow-delete-contract-v1`, `CONTEXT.md`

Result:

- Opened the delete contract lane from current Jellyflow delete helpers and XyFlow source evidence.
- Set `JDC-020` as the first executable task.
- Identified stale model-policy follow-on language as navigation drift, not as absence of runtime
  delete code.
- Kept DOM key handling, async `onBeforeDelete`, renderer UI, screenshots, and pixels outside
  runtime.

Behavior proven:

- Planning artifacts agree on target state, task order, gates, source coverage, and autonomous
  commit policy.

Fresh verification:

- 2026-06-02: `jq empty docs/workstreams/jellyflow-delete-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-delete-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-delete-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-delete-contract-v1/CONTEXT.jsonl` passed.
- 2026-06-02: `git diff --check` passed.

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
