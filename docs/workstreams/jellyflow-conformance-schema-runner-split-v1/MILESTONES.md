# Jellyflow Conformance Schema Runner Split v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- Workstream docs exist and agree.
- Refactor is explicitly behavior-preserving.
- Fixture schema version, serde shape, public re-exports, and runner behavior remain unchanged.

## M1 - Scenario Schema Split

- `runtime::conformance::scenario::mod` is a private facade.
- Suite/scenario builders are separate from setup, action, and trace vocabulary.
- Public conformance exports and fixture JSON shape are unchanged.

## M2 - Runner Split

- `runtime::conformance::runner::mod` is a private facade.
- Action execution is separate from store/gesture trace projection.
- Callback trace recording is separate from suite/scenario orchestration.
- Existing conformance tests, example harness tests, public-surface tests, and clippy pass.

## M3 - Closeout

- Fresh formatting, test, lint, JSON, and diff checks are recorded.
- Follow-ons are split or explicitly deferred.

## Outcome

All milestones are complete. The split stayed inside `jellyflow-runtime::runtime::conformance` and
did not change public API paths, fixture JSON schema, trace ordering, callback payloads, or renderer
boundaries.
