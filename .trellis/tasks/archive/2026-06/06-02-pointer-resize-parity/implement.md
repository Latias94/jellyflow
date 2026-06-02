# Pointer Resize Session Parity Implementation Plan

## Phase 0: Scope Lock

- Confirm first-slice scope: pointer resize math, store entry point, conformance, template smoke.
- Keep XyFlow node-as-parent child correction and lifecycle callbacks out of this task unless scope
  is widened explicitly.

## Phase 1: Pre-Development Context

- Run Trellis before-dev for `jellyflow-runtime`.
- Re-read the runtime resize module, conformance action/runner modules, public-surface test, and
  headless adapter template before editing.

## Phase 2: Red Tests

- Add focused runtime tests for:
  - bottom-right pointer growth,
  - left/top position movement,
  - min/max clamping,
  - keep-aspect-ratio diagonal resize,
  - keep-aspect-ratio single-axis resize,
  - `NodeExtent::Rect`,
  - `NodeExtent::Parent` through group rect,
  - missing-size/hidden/no-op rejection.
- Add conformance coverage for at least one pointer resize mutation and one invalid/no-op case.
- Add public-surface coverage for newly exposed request or store method types.

## Phase 3: Implementation

- Add pointer resize request/options types in `runtime::resize`.
- Add a pure geometry helper that computes pointer-derived `CanvasPoint` and `CanvasSize`.
- Reuse existing transaction planning where possible so target-size and pointer resize produce the
  same operation ordering.
- Add a `NodeGraphStore` method for adapter-facing pointer resize commits.
- Add conformance serialization and runner support.
- Update crate-root exports and template usage intentionally.

## Phase 4: Verification

Run the smallest meaningful gates first:

```text
cargo fmt --check
cargo nextest run -p jellyflow-runtime resize
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo nextest run -p jellyflow-runtime --test public_surface
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
git diff --check
```

Escalate to broader runtime or workspace gates only if public exports, manifests, or shared store
behavior change beyond this capability.

Completed verification:

```text
cargo fmt --check
cargo nextest run -p jellyflow-runtime resize
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo nextest run -p jellyflow-runtime --test public_surface
cargo nextest run -p jellyflow-runtime conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
git diff --check
python3 .trellis/scripts/task.py validate .trellis/tasks/06-02-pointer-resize-parity
```
