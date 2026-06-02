# Directory Structure

`jellyflow-core` uses small module families around the persistent graph model
and undoable operations.

## Layout

```text
crates/jellyflow-core/src/
  lib.rs                 # crate docs and canonical public re-exports
  core/                  # IDs, graph model, imports, symbols, validation
    model/               # Graph, Node, Port, Edge, Group, StickyNote
    validate/            # graph invariant checks and reports
    tests/               # core model/import/symbol/subgraph tests
  interaction/           # renderer-neutral interaction value types
  ops/                   # graph operations, transactions, apply, diff, history
    apply/               # op application and ApplyError
    diff/                # graph diffing and metadata helpers
    fragment/            # clipboard/fragment/remap/paste helpers
    history/             # undo/redo storage and inversion
    mutation/            # checked mutation planners
    normalize/           # transaction normalization/coalescing
    tests/               # operation and diff tests
  types/                 # type descriptors and compatibility checks
```

## Placement Rules

- Put persisted graph fields and invariant validation under `core/`.
- Put undoable graph edits under `ops/`; runtime code should consume these
  transactions instead of mutating graph maps directly.
- Put reusable type-compatibility rules under `types/`.
- Put editor interaction value types that are still model-safe under
  `interaction/`.
- Keep crate-root re-exports intentional. Add them only for stable public entry
  points that consumers should use directly.

## Naming Rules

- Use explicit domain names such as `GraphTransaction`, `GraphOp`,
  `GraphValidationError`, `NodeId`, and `PortId`.
- Preserve `NodeGraph*` names when they are already part of the public
  interaction vocabulary; do not rename public symbols as part of unrelated
  work.
- Keep IDs strongly typed. Do not replace typed IDs with raw strings in new
  APIs.

## Examples To Follow

- `crates/jellyflow-core/src/lib.rs` documents the headless boundary and keeps
  public re-exports centralized.
- `crates/jellyflow-core/src/ops/apply/error.rs` uses a focused domain error
  enum for transaction application failures.
- `crates/jellyflow-core/src/core/validate/` keeps graph invariant checks out
  of runtime behavior.
