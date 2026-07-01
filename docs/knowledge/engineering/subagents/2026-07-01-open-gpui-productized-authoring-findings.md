---
type: "Subagent Finding"
title: "Open GPUI Productized Authoring Findings"
description: "Read-only subagent synthesis for the next Open GPUI authoring maturity plan."
tags: ["engineering-memory", "subagent", "open-gpui", "authoring", "adapter"]
timestamp: 2026-07-01T00:00:00+08:00
status: "active"
related_plan: "../../plans/2026-07-01-001-feat-open-gpui-productized-authoring-plan.md"
---

# Finding

Five read-only subagents reviewed the Open GPUI productized authoring gap from different angles:
adapter authoring, Open GPUI component APIs, visual regression, runtime boundary, and `egui-snarl`
comparison.
They converged on the same next stage: `jellyflow-open-gpui` already has semantic control,
action/menu, inspector, repeatable, measurement, and product fixture contracts, but
`canvas-jellyflow` still lacks a mature live authoring loop.

The next plan should therefore focus on Open GPUI product authoring, not new headless descriptor
types.
Priority gaps are live control edits, action/menu dispatch, dropped-wire insert, repeatable
add/remove/reorder, inspector editing, event arbitration, structured visual/interaction regression,
and stale capability text cleanup.

# Evidence

- `crates/jellyflow-open-gpui/src/controls.rs` already maps semantic controls and can plan
  `GraphTransaction + NodeInternalsInvalidation`, but live Open GPUI controls in
  `repo-ref/open-gpui/examples/canvas-jellyflow/src/main.rs` render values without callback wiring.
- `repo-ref/open-gpui/crates/ui_components/src` already has callback-capable `TextInput`,
  `Textarea`, `Select`, `Switch`, `NumberInput`, `Slider`, `Button`, `Menu`, `Checkbox`, and
  `Popover` primitives, so the first slice should not require broad Open GPUI internals work.
- `crates/jellyflow-open-gpui/src/testing.rs` already provides structured product fixture reports.
  Subagents recommended extending these reports for density, resize, invalid hover, inspector,
  dropped-wire, repeatable mutation, and fallback states instead of making pixel-golden screenshots
  the hard gate.
- Runtime boundary review confirmed that runtime should own semantic descriptors, action intent,
  measurement lifecycle, graph transactions, and conformance vocabulary. GPUI must own widgets,
  focus, popup/menu state, local component callbacks, and visual smoke.
- `repo-ref/egui-snarl` remains useful prior art for authoring ergonomics. Jellyflow should emulate
  its easy custom-node entry point at the Open GPUI adapter layer, not by exposing toolkit UI in
  runtime.

# Recommendation

Use `docs/plans/2026-07-01-001-feat-open-gpui-productized-authoring-plan.md` as the next
implementation-ready plan.
The plan should execute in Open GPUI scope only, keeping egui/Dioxus expansion deferred.
If implementation reveals duplicated binding-write logic across adapters, consider promoting a
renderer-neutral authoring planner to runtime later, but do not move widget state or GPUI types
upstream.

# Disposition

Captured into the new implementation-ready plan.
No code was changed by the subagents.
