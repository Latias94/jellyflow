# Jellyflow Delete Contract v1 - Evidence And Gates

Status: Closed
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

### 2026-06-02 - JDC-020 Template Delete Smoke

Scope: `templates/headless-adapter`, `docs/workstreams/jellyflow-delete-contract-v1`

Result:

- Added a template `delete selection` scenario to the built-in headless adapter suite.
- Added `run_delete_selection_smoke` as a single-scenario template smoke helper.
- Used `ConformanceAction::apply_delete_selection_for_key` with Backspace, matching XyFlow-style
  key-bound delete flow after adapter input normalization.
- Added a connected node/edge fixture so deleting the selected node proves cascaded edge deletion,
  disconnect callbacks, delete callbacks, and selection cleanup.
- Updated template README and suite assertions from 6 to 7 scenarios.

Behavior proven:

- Template delete trace records `delete selection` / `remove_node`.
- XyFlow-style callbacks record node/edge changes, disconnect, nodes/edges delete, combined delete,
  and empty selection change.
- Runtime conformance and adapter conformance delete behavior still pass.

Fresh verification:

- 2026-06-02: `cargo fmt --check` passed.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime conformance` passed, 55 tests run.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime adapter_conformance` passed, 18 tests run.
- 2026-06-02: `cargo test --manifest-path templates/headless-adapter/Cargo.toml` passed, 10 tests run.
- 2026-06-02: `cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check` passed and reported matching built-in suite traces.

### 2026-06-02 - JDC-030 Documentation And Closeout

Scope: `README.md`, `crates/jellyflow-runtime/README.md`, `CONTEXT.md`,
`docs/workstreams/jellyflow-delete-contract-v1`

Result:

- Documented `runtime::delete` and `runtime::keyboard` in the root and runtime README interaction
  contracts.
- Documented runtime ownership of delete selection planning, configured delete-key gating,
  effective policy checks, cascaded edge deletion, XyFlow-style callbacks, and selection cleanup.
- Documented adapter ownership of platform key capture, focus/input suppression, confirmation
  dialogs, async pre-delete hooks, renderer feedback, screenshots, and pixels.
- Cleared stale "delete planner ownership" navigation from `CONTEXT.md`.
- Closed workstream metadata, task ledger, campaign record, handoff, and closeout audit.

Behavior proven:

- Template and runtime conformance evidence already prove delete commit and callback traces.
- Headless crates still avoid renderer, DOM, d3, wgpu, egui, Fret, screenshot, and pixel
  dependencies.

Fresh verification:

- 2026-06-02: `cargo fmt --check` passed.
- 2026-06-02: `cargo nextest run -p jellyflow-runtime` passed, 291 tests run.
- 2026-06-02: `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings` passed.
- 2026-06-02: `jq empty docs/workstreams/jellyflow-delete-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-delete-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-delete-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-delete-contract-v1/CONTEXT.jsonl` passed.
- 2026-06-02: `git diff --check` passed.

## Notes

Fresh command evidence must be appended here before any task or lane completion claim.
