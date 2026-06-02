# Jellyflow Visible Elements Contract v1 - Milestones

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

Exit criteria:

- workstream artifacts agree on visible node v1 scope;
- source coverage names XyFlow visible node ids, prior geometry/spatial evidence, and current
  Jellyflow viewport/bounds utilities;
- `CONTEXT.md` points at this active workstream.

Status: complete on 2026-06-02. JVE-020 is the first executable task.

## M1 - Visible Node Runtime Contract

Exit criteria:

- runtime exposes a renderer-neutral visible node id request/plan;
- store helper uses current view state and resolved rendering interaction;
- culling disabled returns all eligible node ids deterministically;
- culling enabled returns partially visible node ids from viewport bounds;
- public surface smoke proves the module is externally reachable.

Status: complete on 2026-06-02. Implementation landed in `runtime::rendering` beside existing
renderer-neutral render-order helpers.

## M2 - Conformance And Template Coverage

Exit criteria:

- conformance fixtures can assert visible node ids;
- template adapter suite includes a visible node smoke scenario;
- focused conformance/template gates pass.

Status: complete on 2026-06-02. `assert_visible_node_ids` is available in conformance fixtures and
the headless adapter template suite includes a visible-node scenario.

## M3 - Documentation And Closeout

Exit criteria:

- README/runtime README document visible node runtime/adapter boundaries;
- `CONTEXT.md` records visible node contract as closed and keeps real spatial index/visible edge
  culling as follow-ons;
- package, clippy, JSON, and diff gates pass.

Status: pending.
