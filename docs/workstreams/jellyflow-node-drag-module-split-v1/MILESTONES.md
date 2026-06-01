# Jellyflow Node Drag Module Split v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- Workstream docs exist and agree.
- Refactor is explicitly behavior-preserving.
- Parent expansion, resize, adapter, and renderer behavior remain out of scope.

## M1 - Module Split

- `runtime::drag::mod` is a small facade.
- Public drag types are separate from planner execution.
- Candidate filtering is separate from constraint math.
- Store extension methods are separate from pure planning helpers.
- Existing drag tests, conformance tests, public-surface tests, and clippy pass.

## M2 - Closeout

- Fresh formatting, test, lint, JSON, and diff checks are recorded.
- Follow-ons are split or explicitly deferred.

## Outcome

All milestones are complete. The code split stayed inside `jellyflow-runtime::runtime::drag` and did
not change public API paths, drag behavior, conformance fixture semantics, or renderer boundaries.
