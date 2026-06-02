# Resize Lifecycle Callback Implementation Plan

## Checklist

- Read the runtime specs, ADR 0004, this task research, and the archived pointer resize task before
  editing code.
- Add `runtime/events/node_resize.rs` and re-export the new types.
- Add resize variants to `NodeGraphGestureEvent`.
- Extend `NodeGraphGestureCallbacks`, callback dispatch, and callback type re-exports.
- Extend `ConformanceCallbackEvent` and `ConformanceCallbackTraceRecorder`.
- Add a conformance runner test for resize lifecycle ordering around pointer resize.
- Update the headless adapter template resize smoke scenario to include lifecycle events.
- Update `crates/jellyflow-runtime/tests/public_surface.rs` for public event/callback exposure.
- Run formatting and focused tests.
- Update PRD acceptance checkboxes after verification.

## Validation Commands

```bash
cargo fmt --check
cargo nextest run -p jellyflow-runtime conformance_runner_executes_node_resize_lifecycle_fixture_and_matches_trace
cargo nextest run -p jellyflow-runtime conformance_module_exposes_serde_friendly_headless_fixture_vocabulary
cargo nextest run -p jellyflow-runtime explicit_modules_expose_their_owned_surfaces
cargo test --manifest-path templates/headless-adapter/Cargo.toml
python3 .trellis/scripts/task.py validate .trellis/tasks/06-02-resize-lifecycle-callbacks
```

If the focused test filter names differ after implementation, run the nearest focused
`jellyflow-runtime` conformance/public-surface checks and record the exact commands in the final
task notes.

## Risk Points

- `NodeGraphGestureEvent` is serialized; use the existing serde tag/content style.
- Callback trait changes should use default methods only, preserving existing implementors.
- Do not embed planner internals in lifecycle event payloads; use public graph geometry fields.
- Do not add session state or `shouldResize` behavior in this slice.
