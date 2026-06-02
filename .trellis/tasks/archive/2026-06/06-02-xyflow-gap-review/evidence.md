# Evidence

## Source Inspection

Reviewed XyFlow reference code under `repo-ref/xyflow`:

- `packages/system/src/xydrag/XYDrag.ts`
- `packages/system/src/xyhandle/XYHandle.ts`
- `packages/system/src/xypanzoom/XYPanZoom.ts`
- `packages/system/src/xyresizer/XYResizer.ts`
- `packages/system/src/utils/graph.ts`
- `packages/system/src/utils/store.ts`
- `packages/system/src/utils/edges/*`
- `packages/react/src/store/initialState.ts`
- `packages/react/src/utils/changes.ts`
- selected React hooks/components for delete, fitView, visible ids, keyboard movement,
  node focus auto-pan, wrappers, minimap, controls, background, toolbar, providers,
  and DOM measurement.

Reviewed Jellyflow code:

- `crates/jellyflow-core/src/core/model/*`
- `crates/jellyflow-core/src/ops/transaction/*`
- `crates/jellyflow-runtime/src/io/config/interaction/config.rs`
- `crates/jellyflow-runtime/src/runtime/drag/*`
- `crates/jellyflow-runtime/src/runtime/resize/*`
- `crates/jellyflow-runtime/src/runtime/connection/*`
- `crates/jellyflow-runtime/src/runtime/delete/*`
- `crates/jellyflow-runtime/src/runtime/keyboard/*`
- `crates/jellyflow-runtime/src/runtime/selection/*`
- `crates/jellyflow-runtime/src/runtime/viewport/*`
- `crates/jellyflow-runtime/src/runtime/fit_view/*`
- `crates/jellyflow-runtime/src/runtime/auto_pan/*`
- `crates/jellyflow-runtime/src/runtime/geometry/*`
- `crates/jellyflow-runtime/src/runtime/rendering/*`
- `crates/jellyflow-runtime/src/runtime/xyflow/*`
- `crates/jellyflow-runtime/src/runtime/conformance/*`
- `templates/headless-adapter/src/lib.rs`

Reviewed boundary docs:

- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `CONTEXT.md`
- `.trellis/spec/guides/index.md`
- `.trellis/spec/repository/backend/index.md`
- `.trellis/spec/jellyflow-core/backend/index.md`
- `.trellis/spec/jellyflow-runtime/backend/index.md`

## Output

- Added `docs/reviews/xyflow-gap-2026-06-02.md`.
- No runtime/core source files were changed.

## Validation

- `python3 ./.trellis/scripts/task.py validate 06-02-xyflow-gap-review`
  - Passed with 6 `implement.jsonl` entries and 4 `check.jsonl` entries.
- `git add -N .trellis/tasks/06-02-xyflow-gap-review docs/reviews/xyflow-gap-2026-06-02.md && git diff --check`
  - Passed.

Rust behavior tests were not run because this task changed only review/Trellis
documentation and did not modify runtime/core behavior, public APIs, fixtures,
or adapter template code.

## Spec Update Judgment

Ran the Trellis spec-update review. No `.trellis/spec/` update was needed because
this task did not introduce new commands, APIs, schema fields, cross-layer
contracts, implementation conventions, or reusable coding patterns. The durable
knowledge from the review is captured in `docs/reviews/xyflow-gap-2026-06-02.md`
instead.
