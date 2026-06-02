# Resize model ADR

## Goal

Write an ADR that fixes the next resize-model decision after pointer resize parity:
Jellyflow should not silently copy XyFlow's node-as-parent child correction into the current v1
model, because Jellyflow containment is group-based today. The ADR should also define where future
resize lifecycle callbacks belong.

## Requirements

- Capture the current evidence from ADR 0001, ADR 0002, ADR 0003, the XyFlow gap review, and the
  pointer resize parity task.
- Decide whether Jellyflow v1 should introduce first-class node-owned child containment for
  XyFlow `XYResizer` child correction.
- Decide how future resize lifecycle callbacks should be layered.
- Preserve the existing headless boundary: no renderer, DOM, React, Fret UI, `wgpu`, or `winit`
  dependency decisions inside the ADR.
- Update the ADR index.
- Keep this as a documentation task; do not change runtime code.

## Acceptance Criteria

- [x] A new ADR is added under `docs/adr/` with the next zero-padded number.
- [x] The ADR states the accepted decision and the consequences.
- [x] The ADR distinguishes group containment from XyFlow node-as-parent child correction.
- [x] The ADR places resize lifecycle callbacks under runtime/xyflow/conformance, not core model
  storage or adapter UI code.
- [x] `docs/adr/README.md` links the ADR.
- [x] `git diff --check` passes.

## Notes

- User approved creating a Trellis task and writing the ADR.
- Recommended decision: keep Jellyflow v1 group-based containment unchanged, defer exact
  XyFlow node-as-parent child correction to a separate model/schema ADR if it becomes necessary,
  and add resize lifecycle callback parity as a runtime/xyflow/conformance follow-up.
