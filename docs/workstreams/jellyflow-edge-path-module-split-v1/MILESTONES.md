# Jellyflow Edge Path Module Split v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- Workstream docs exist and agree.
- Refactor is explicitly behavior-preserving.
- Public geometry re-exports, path commands, labels, and routing outputs remain unchanged.

## M1 - Module Split

- `runtime::geometry::paths::mod` is a private facade.
- Public path types are separate from straight, bezier, smoothstep, and label helper logic.
- Existing geometry path tests, public-surface tests, package tests, and clippy pass.

## M2 - Closeout

- Fresh formatting, test, lint, JSON, and diff checks are recorded.
- Follow-ons are split or explicitly deferred.

## Outcome

All milestones are complete. The split stayed inside `jellyflow-runtime::runtime::geometry::paths`
and did not change public API paths, edge path commands, label placement, smoothstep routing,
bezier control behavior, hit testing, or renderer boundaries.
