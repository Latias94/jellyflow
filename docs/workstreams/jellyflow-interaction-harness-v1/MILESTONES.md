# Jellyflow Interaction Harness v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Harness Contract

Exit criteria:

- The workstream documents agree on problem, non-goals, source coverage, and first task.
- ADR 0003 is the governing architecture decision.

## M1 - Runtime Trace Harness

Exit criteria:

- Runtime tests have a reusable harness around a real `NodeGraphStore`.
- The harness records normalized graph, view, and gesture events.
- At least one adapter-conformance scenario uses the harness assertions.
- Targeted nextest and `cargo check` pass.

## M2 - Fixture-Driven Selection Kernel

Exit criteria:

- Selection-box behavior is expressible as a renderer-neutral fixture.
- Fixture assertions cover selection output and event ordering.
- Policy and hidden-node behavior are explicit.

## M3 - Gesture Kernel Fixtures

Exit criteria:

- At least one more gesture family uses the harness.
- The fixture trace includes graph transactions and compatibility projections where relevant.
- Failure output remains compact enough for agent debugging.

## M4 - Closeout

Exit criteria:

- Runtime package gates pass.
- Workstream evidence is current.
- Follow-ons are split for public fixture format, drag/pan/resize kernels, or renderer smoke tests.

Completion:

- JIH-050 closed the lane after README/runtime README documentation, closeout evidence, and fresh
  runtime package gates.
- Public fixture format, richer drag/pan/resize kernels, and renderer smoke tests remain separate
  follow-ons.
