# Jellyflow Node Drag Kernel v1 - Handoff

Status: Active
Last updated: 2026-06-01

## Current State

The workstream is open as a follow-on to the closed interaction harness lane.

JND-010 is complete: the lane scope, non-goals, source coverage, task ledger, milestones, gate set,
context manifest, and machine-readable workstream metadata are recorded.

JND-020 is complete: `runtime::drag` now exposes the first single-node drag planning/apply helper.
The helper converts a canvas-space target into a labeled `SetNodePos` transaction through normal
`NodeGraphStore` dispatch. Harness-backed fixtures prove successful commit traces and no-commit
behavior for missing, hidden, non-draggable, no-op, non-finite, and global-disabled cases.

## Next Task

JND-030: extend drag planning to build deterministic multi-selection drag items from the primary
node plus selected nodes.

## Decisions Since Last Update

- Keep runtime drag renderer-free; adapters own pointer capture, DOM/class filtering, windows, and
  rendering.
- Start with graph transaction planning and committed state before broadening callback/event
  surface.
- Use `GraphOp::SetNodePos` through normal `NodeGraphStore` dispatch for drag commits.
- Use existing policy resolution for `draggable` and node extents.
- Treat public fixture format, auto-pan, and renderer smoke tests as follow-ons unless this lane
  explicitly pulls one in later.
- The first apply helper records normal store history. A later drag-session task can decide whether
  continuous pointer updates need separate preview/final-commit semantics.

## Blockers

- None known.

## Follow-On Candidates

- Auto-pan as a separate headless viewport-plus-drag contract.
- Parent expansion behavior if group resizing semantics need a dedicated lane.
- Public fixture format after drag, connect, and at least one more gesture family settle.
- Renderer adapter smoke tests in future wgpu, egui, Fret, or other adapter crates.
