# Pointer Resize Session Parity Research

## Local Evidence

- `docs/reviews/xyflow-gap-2026-06-02.md` identifies node resize as the largest runtime semantics
  gap: XyFlow has pointer resize sessions, keep-aspect-ratio math, node extents, child correction,
  parent expansion handling, and lifecycle callbacks; Jellyflow currently has target-size resize
  planning only.
- `repo-ref/xyflow/packages/system/src/xyresizer/XYResizer.ts` starts from pointer position,
  measured node geometry, node origin, min/max limits, optional aspect-ratio lock, node extent,
  parent node, and children with `extent: parent` or `expandParent`; updates emit changed
  x/y/width/height fields and child correction changes.
- `repo-ref/xyflow/packages/system/src/xyresizer/utils.ts` contains the relevant pure resize math:
  direction signs, pointer deltas, min/max clamps, node-extent clamps, child-extent clamps, and
  aspect-ratio reconciliation.
- `crates/jellyflow-runtime/src/runtime/resize/planner.rs` currently plans from explicit target
  size. It clamps min/max constraints, resolves node origin, adjusts position by direction, and
  emits `SetNodePos` plus `SetNodeSize`.
- `crates/jellyflow-runtime/src/runtime/resize/types.rs` currently exposes
  `NodeResizeConstraints`, `NodeResizeDirection`, `NodeResizeRequest`, `NodeResizePlan`, and
  `NodeResizeItem`. There is no pointer-session request or aspect-ratio flag yet.
- `crates/jellyflow-core/src/core/model/node.rs` defines `NodeExtent::{Parent, Rect}` and
  `Node.parent: Option<GroupId>`. `crates/jellyflow-core/src/core/model/resources.rs` defines
  `Group.rect` in canvas space.
- `crates/jellyflow-runtime/src/runtime/drag/constraints/extent.rs` already resolves
  `NodeExtent::Rect` and `NodeExtent::Parent` against group rects for drag, with parent extents
  disabled when `expand_parent` is true.
- `crates/jellyflow-runtime/src/runtime/conformance` currently serializes target-size resize only.
  Callback traces do not include resize lifecycle events.
- `templates/headless-adapter/src/lib.rs` currently smokes only target-size bottom-right resize.

## Scope Implications

- The first useful Jellyflow slice is a pure pointer resize solver plus store/conformance entry
  points. Renderer and DOM mechanics belong to adapters.
- XyFlow child correction is not a direct port because Jellyflow uses groups for editor
  containment, not node-owned child nodes. Treating that as first-slice scope would force a broader
  data-model or adapter decision.
- Resize lifecycle callbacks are a separate adapter contract. They can build on pointer planning
  after the math and store commit semantics are stable.
