# XyFlow Gap Review Implementation Plan

## Checklist

- [x] Resolve the React/UI scope question.
- [x] Start the task after planning approval.
- [x] Load `trellis-before-dev` and read applicable repository/core/runtime
      specs.
- [x] Inventory XyFlow source areas:
      - `packages/system/src/types`
      - `packages/system/src/utils`
      - `packages/system/src/xydrag`
      - `packages/system/src/xyhandle`
      - `packages/system/src/xypanzoom`
      - `packages/system/src/xyresizer`
      - `packages/react/src/store`
      - selected React hooks/renderers/components that define behavior.
- [x] Inventory Jellyflow source areas:
      - `crates/jellyflow-core/src/core`
      - `crates/jellyflow-core/src/ops`
      - `crates/jellyflow-runtime/src/io`
      - `crates/jellyflow-runtime/src/rules`
      - `crates/jellyflow-runtime/src/runtime`
      - `crates/jellyflow-runtime/src/schema`
      - `templates/headless-adapter`
- [x] Build a coverage matrix.
- [x] Write `docs/reviews/xyflow-gap-2026-06-02.md`.
- [x] Record review evidence and command notes in the task.
- [x] Run `git diff --check`.
- [x] Run targeted tests only if the report relies on fresh behavioral evidence
      beyond source/test inspection.
- [ ] Commit the review and task artifacts.

## Validation Commands

```text
python3 ./.trellis/scripts/get_context.py --mode packages
python3 ./.trellis/scripts/task.py validate 06-02-xyflow-gap-review
git diff --check
```

Optional fresh behavior gates if needed:

```text
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo nextest run -p jellyflow-runtime conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
```

## Files Likely To Change

- `docs/reviews/xyflow-gap-2026-06-02.md`
- `.trellis/tasks/06-02-xyflow-gap-review/*`

No source files should change unless the user explicitly expands scope.

## Follow-Up Check Before Start

Planning is ready after the user confirms whether React UI components are
adapter-owned inventory or first-class parity targets for this review.
