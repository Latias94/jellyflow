# Jellyflow Node Drag Kernel v1 - Handoff

Status: Active
Last updated: 2026-06-01

## Current State

The workstream is open as a follow-on to the closed interaction harness lane.

JND-010 is complete: the lane scope, non-goals, source coverage, task ledger, milestones, gate set,
context manifest, and machine-readable workstream metadata are recorded.

## Next Task

JND-020: add the first renderer-neutral single-node drag helper and fixture.

## Decisions Since Last Update

- Keep runtime drag renderer-free; adapters own pointer capture, DOM/class filtering, windows, and
  rendering.
- Start with graph transaction planning and committed state before broadening callback/event
  surface.
- Use `GraphOp::SetNodePos` through normal `NodeGraphStore` dispatch for drag commits.
- Use existing policy resolution for `draggable` and node extents.
- Treat public fixture format, auto-pan, and renderer smoke tests as follow-ons unless this lane
  explicitly pulls one in later.

## Blockers

- None known.

## Follow-On Candidates

- Auto-pan as a separate headless viewport-plus-drag contract.
- Parent expansion behavior if group resizing semantics need a dedicated lane.
- Public fixture format after drag, connect, and at least one more gesture family settle.
- Renderer adapter smoke tests in future wgpu, egui, Fret, or other adapter crates.
