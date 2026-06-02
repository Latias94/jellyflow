# Directory Structure

`jellyflow-runtime` groups headless editor behavior by runtime capability. New
modules should preserve those capability boundaries instead of becoming generic
utility buckets.

## Layout

```text
crates/jellyflow-runtime/src/
  lib.rs                 # crate docs and canonical public re-exports
  io/                    # persisted graph/editor files, config, view state
  profile/               # graph profile and apply pipeline hooks
  rules/                 # connection/delete/insert/reconnect planning rules
  runtime/               # store and headless interaction capabilities
    auto_pan/
    conformance/         # fixture vocabulary, runner, reports, approval
    connection/
    delete/
    drag/
    events/
    fit_view/
    geometry/
    keyboard/
    lookups/
    policy/
    rendering/
    resize/
    selection/
    store/
    viewport/
    xyflow/              # explicit XyFlow-compatible vocabulary
  schema/                # registry and migration hooks
```

## Placement Rules

- Put persisted config and file I/O under `io/`.
- Put graph-rule planning that returns transactions/diagnostics under `rules/`.
- Put store-facing interaction helpers under the matching `runtime/<capability>/`
  module.
- Put XyFlow-shaped changes, callbacks, controlled-mode projections, and adapter
  vocabulary under `runtime::xyflow`.
- Put reusable headless adapter assertions under `runtime::conformance`.
- Put tests under the closest existing `runtime/tests/<capability>/`,
  `io/tests/`, `schema/tests/`, or public-surface test.

## Public API Rules

- Crate-root re-exports are canonical runtime entry points. Keep them sparse.
- Add public-surface coverage when exposing a new adapter-facing type or method.
- Prefer methods on `NodeGraphStore` when a helper needs current graph, view
  state, policy, lookups, or commit behavior.
- Keep pure calculation helpers public only when external adapters need them.

## Examples To Follow

- `crates/jellyflow-runtime/src/lib.rs` documents crate boundaries and public
  re-export intent.
- `crates/jellyflow-runtime/src/runtime/rendering/` composes visibility and
  render-order behavior before adapters render.
- `crates/jellyflow-runtime/src/runtime/conformance/` keeps renderer-independent
  adapter scenarios reusable.
- `crates/jellyflow-runtime/src/runtime/xyflow/` isolates compatibility naming.
