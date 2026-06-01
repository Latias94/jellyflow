# Jellyflow Runtime Test Surface Split v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- Workstream docs exist and agree.
- Refactor is explicitly test-only and behavior-preserving.
- Runtime production code, fixture schema, public APIs, and renderer boundaries remain out of scope.

## M1 - Conformance And Adapter Test Surface

- Conformance tests are grouped by scenario family or trace responsibility.
- Adapter-facing conformance tests separate setup, action, callback, and assertion concerns where
  the existing file shape is too broad.
- Harness helper extraction stays local to runtime tests.
- Conformance, example harness, and public-surface gates pass.

## M2 - Drag And Runtime Interaction Test Surface

- Drag tests are grouped by scenario family and local helper ownership.
- Interaction-heavy runtime tests keep selection, viewport, auto-pan, dispatch, and callback
  behavior stable.
- Targeted and package gates pass.

## M3 - Closeout

- Fresh formatting, test, lint, JSON, and diff checks are recorded.
- Remaining core-test or future gesture work is split or explicitly deferred.
- `WORKSTREAM.json` status is updated.

## Closeout Standard

This lane closes only after evidence shows that the split did not alter runtime behavior, public
surface, fixture JSON shape, conformance traces, or the renderer-free headless boundary.

## Outcome

All milestones are complete. The selected runtime test surfaces now use focused scenario/support
modules, final package and lint gates passed, and future core-test organization or new gesture
behavior remains split into separate follow-on work.
