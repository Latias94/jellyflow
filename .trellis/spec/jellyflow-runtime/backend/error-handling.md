# Error Handling

Runtime errors should preserve the difference between rejected user/editor
intent, failed graph transaction application, and file/conformance failures.

## Patterns

- Use `thiserror::Error` for public or reusable runtime error enums.
- Return diagnostics from rules when adapter feedback needs a structured reason.
- Use `ApplyPipelineError` for profile/apply pipeline failures.
- Use capability-specific errors for store helpers such as connect, reconnect,
  delete, keyboard, file I/O, conformance, and schema migration.
- Use `String` only for internal assertion helpers where the failure is already
  scoped to a conformance runner check.

## Existing Error Families

- `profile::ApplyPipelineError` for graph apply/profile pipeline failures.
- `runtime::store::DispatchError` for store dispatch failures.
- `runtime::connection::{ConnectEdgeError, ReconnectEdgeError}` for connection
  store helpers.
- `runtime::delete::DeleteSelectionError` and `runtime::keyboard` errors for
  normalized interaction helpers.
- `io::files::*Error` for graph/editor state file load/save/validate failures.
- `runtime::conformance` report and fixture file errors for suite execution and
  approval.
- `runtime::xyflow` errors for change-to-transaction and controlled-mode
  compatibility paths.

## What Not To Do

- Do not convert diagnostics into opaque strings for public adapter APIs.
- Do not panic for invalid editor intent; reject it through a typed error or
  diagnostic.
- Do not mix renderer/platform input errors into runtime errors. Adapters should
  normalize input before calling runtime.
- Do not make XyFlow compatibility errors the canonical error shape for non-
  XyFlow runtime modules.

## Tests

For rejected interaction intent, test both the outcome and the diagnostic/error
shape. For conformance failures, prefer compact expected mismatch reports over
stringly checking unrelated formatting.
