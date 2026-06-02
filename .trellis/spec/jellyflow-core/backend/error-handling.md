# Error Handling

Core errors describe graph model and transaction failures. They should be
specific enough for runtime code and tests to distinguish causes without parsing
strings.

## Patterns

- Use `thiserror::Error` for public or reusable domain errors.
- Prefer enum variants with typed IDs and structured fields.
- Keep validation errors separate from mutation/application errors.
- Return `Result<T, ErrorType>` from fallible helpers instead of panicking.
- Use `Vec<GraphValidationError>` only when the API intentionally reports
  multiple invariant failures.

## Existing Error Families

- `core::imports::GraphImportError` for import closure failures.
- `core::validate::GraphValidationError` for graph invariant violations.
- `ops::apply::ApplyError` for applying `GraphOp`/`GraphTransaction` to a
  graph.
- `ops::mutation::GraphMutationError` for checked mutation planning.
- `core::symbol_ref` and `core::subgraph` errors for binding/reference checks.

## What Not To Do

- Do not introduce HTTP/API response concepts in this crate.
- Do not hide domain failures behind `String` for new public APIs.
- Do not use `unwrap` or `expect` in production paths to enforce graph
  invariants; return a typed error.
- Do not make runtime policy or adapter callback errors part of core.

## Tests

When adding an error branch, add a focused test that exercises the exact branch
or verifies the resulting validation report. Prefer comparing enum variants over
matching formatted error text.
