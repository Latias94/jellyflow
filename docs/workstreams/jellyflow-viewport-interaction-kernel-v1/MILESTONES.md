# Jellyflow Viewport Interaction Kernel v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

Exit criteria:

- Workstream documents agree on viewport interaction scope and renderer non-goals.
- ADR 0003 remains the renderer-boundary decision.
- Conformance fixture closeout is treated as source evidence.

## M1 - Viewport Kernel

Exit criteria:

- Runtime exposes deterministic pan/zoom request types and transform helpers.
- Helpers express drag-pan and zoom-around-pointer from normalized adapter intent.
- Public surface or focused tests protect the API shape.

## M2 - Store Gesture And Callbacks

Exit criteria:

- `NodeGraphStore` can apply viewport intent through view-state publication.
- Gesture lifecycle payloads and XyFlow-compatible move callbacks are covered.
- Targeted nextest and `cargo check` pass.

## M3 - Conformance Fixtures

Exit criteria:

- Viewport conformance scenarios execute through `run_conformance_scenario`.
- Adapter conformance keeps viewport ordering coverage through fixture traces where appropriate.
- Conformance and adapter-conformance gates pass.

## M4 - Documentation And Closeout

Exit criteria:

- Runtime package gates pass.
- README material explains viewport fixture conformance before renderer smoke tests.
- Workstream evidence is current.
- Follow-ons are split for animation, auto-pan, adapter runner helpers, or renderer smoke tests if
  they remain out of scope.
