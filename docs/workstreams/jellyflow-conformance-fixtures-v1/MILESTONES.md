# Jellyflow Conformance Fixtures v1 - Milestones

Status: Active
Last updated: 2026-06-01

## M0 - Scope And Contract

Exit criteria:

- The workstream documents agree on problem, non-goals, source coverage, and first task.
- ADR 0003 remains the governing renderer-boundary decision.
- The node drag and interaction harness closeouts are treated as source evidence.

## M1 - Fixture Vocabulary

Exit criteria:

- Runtime has a renderer-free fixture vocabulary for setup, actions, gestures, expected traces, and
  mismatch reporting.
- Fixture types can express existing connect and node drag conformance scenarios.
- Public surface coverage protects the exported API shape.

## M2 - Fixture Runner

Exit criteria:

- Runtime has a runner that executes fixtures against a real `NodeGraphStore`.
- Failure output identifies the scenario, step, expected trace, and actual trace.
- Targeted nextest and `cargo check` pass.

## M3 - Existing Scenario Conversion

Exit criteria:

- Existing connect and node drag adapter-conformance behavior remains covered through the fixture
  runner.
- The old harness remains available only where it still adds value.
- Adapter conformance and conformance runner gates pass.

## M4 - Documentation And Closeout

Exit criteria:

- Runtime package gates pass.
- Workstream evidence is current.
- README material explains when to use fixture conformance versus renderer smoke tests.
- Follow-ons are split for file-backed golden fixtures, adapter crate runners, or broader gesture
  families if they remain out of scope.
