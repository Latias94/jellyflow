# Jellyflow Node Drag Kernel v1 - Handoff

Status: Closed
Last updated: 2026-06-01

## Current State

The workstream is closed. It was opened as a follow-on to the closed interaction harness lane.

JND-010 is complete: the lane scope, non-goals, source coverage, task ledger, milestones, gate set,
context manifest, and machine-readable workstream metadata are recorded.

JND-020 is complete: `runtime::drag` now exposes the first single-node drag planning/apply helper.
The helper converts a canvas-space target into a labeled `SetNodePos` transaction through normal
`NodeGraphStore` dispatch. Harness-backed fixtures prove successful commit traces and no-commit
behavior for missing, hidden, non-draggable, no-op, non-finite, and global-disabled cases.

JND-030 is complete: drag planning now builds ordered `NodeDragItem` output from the primary node
plus current selected nodes. It co-drags selected nodes with the primary node, filters hidden and
non-draggable nodes, filters child nodes whose parent group is selected, and commits sorted
`SetNodePos` ops.

JND-040 is complete: drag planning now applies snap-to-grid and movement extent constraints without
renderer dependencies. Multi-selection drag uses one shared snap offset from the first deterministic
drag item. Global `node_extent` clamps a multi-selection as a group, per-node rect extents clamp
individual nodes, and node-origin-aware bounds are used for extent math. `NodeExtent::Parent` is
resolved to the parent group rect when `expand_parent` is false.

JND-050 is complete: the runtime now exposes renderer-neutral node drag gesture payloads for
start/update/end with pointer intent. The interaction harness records these events, the
XyFlow-compatible callback adapter dispatches node drag start/update/end callbacks, and adapter
conformance verifies ordering around a committed drag transaction and `NodeChange::Position`
projection.

JND-060 is complete: README material documents the drag kernel strategy, closeout evidence is
recorded, the workstream is closed, and follow-ons are split below.

## Next Task

None in this workstream. Open a new lane or workstream for the follow-ons below.

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
- Jellyflow's selected-parent filtering currently maps to node `parent: GroupId` plus selected
  groups, not XyFlow node-parent nesting.
- JND-040 keeps parent expansion out of scope. Parent extent can constrain to the current group
  rect, but automatic group resizing should remain a follow-on if needed.
- JND-050 keeps renderer pointer capture out of scope. Adapters emit already-normalized canvas
  pointer payloads into the runtime gesture stream.

## Blockers

- None known.

## Follow-On Candidates

- Auto-pan as a separate headless viewport-plus-drag contract.
- Parent expansion behavior if group resizing semantics need a dedicated lane.
- Public fixture format after drag, connect, and at least one more gesture family settle.
- Renderer adapter smoke tests in future wgpu, egui, Fret, or other adapter crates.
