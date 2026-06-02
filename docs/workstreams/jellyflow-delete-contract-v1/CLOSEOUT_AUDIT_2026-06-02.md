# Jellyflow Delete Contract v1 - Closeout Audit

Date: 2026-06-02

## Final Status

Closed.

## Completed Scope

- Promoted existing `runtime::delete` and `runtime::keyboard` helpers into a documented headless
  adapter contract.
- Added key-bound delete selection smoke coverage to `templates/headless-adapter`.
- Proved cascaded edge deletion, XyFlow-style disconnect/delete callbacks, and selection cleanup
  through conformance traces.
- Updated root/runtime docs and project context to remove stale delete planner follow-on wording.

## Gates

All passed on 2026-06-02:

- `cargo fmt --check`
- `cargo nextest run -p jellyflow-runtime`
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
- `jq empty docs/workstreams/jellyflow-delete-contract-v1/WORKSTREAM.json docs/workstreams/jellyflow-delete-contract-v1/TASKS.jsonl docs/workstreams/jellyflow-delete-contract-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-delete-contract-v1/CONTEXT.jsonl`
- `git diff --check`

## Follow-Ons

- Async pre-delete or confirmation-dialog parity only after adapter evidence proves direct
  `runtime::delete` or `runtime::keyboard` calls are insufficient.
- Renderer smoke remains in future adapter crates, not in `jellyflow-core` or `jellyflow-runtime`.
- Schema migration for persisted `deletable` fields remains separate from delete behavior.

## Residual Risk

The runtime now owns deterministic selection deletion for the stable headless contract. It does not
model XyFlow's async `onBeforeDelete` hook or renderer confirmation UI. That split is intentional:
adapters can run those policies before calling Jellyflow until evidence justifies a structured
headless pre-delete request.
