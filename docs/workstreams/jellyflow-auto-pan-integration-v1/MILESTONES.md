# Jellyflow Auto-Pan Integration v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

Exit criteria:

- Workstream docs exist and agree.
- ADR 0003 renderer-free boundary is explicitly referenced.
- Follow-ons from node drag and viewport lanes are narrowed to auto-pan.

## M1 - Runtime Kernel

Exit criteria:

- Public `runtime::auto_pan` types are available from `jellyflow-runtime`.
- Focused tests cover disabled policy, invalid input, edge intensity, sign convention, and store
  view-state publication.
- Public-surface test can construct and run the auto-pan API.

## M2 - Conformance Fixture

Exit criteria:

- Conformance scenarios can apply one auto-pan frame.
- Trace assertions prove viewport change and XyFlow-style viewport callback ordering.
- Adapter conformance coverage uses the fixture runner where appropriate.

## M3 - Closeout

Exit criteria:

- README/runtime README explain that runtime owns deterministic auto-pan math and adapters own frame
  scheduling/input capture.
- Fresh formatting, test, lint, JSON, and diff checks are recorded.
- Follow-ons are split or explicitly deferred.
