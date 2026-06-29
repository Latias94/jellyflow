---
type: "Session Handoff"
title: "Semantic surface and node-kit gap"
description: "Handoff for the next adapter planning step after confirming the headless semantic-surface direction."
tags: ["engineering-memory", "jellyflow", "session", "adapter", "node-kit"]
timestamp: 2026-06-19T15:41:22Z
status: "active"
---

# Summary

Jellyflow's current headless direction is correct: keep the graph/runtime semantic and adapter-owned, not widget-owned. ADR 0008 already captures the durable boundary. The next gap is not a shared UI crate; it is a reusable adapter/node-kit layer that can map the same semantic surface into egui, Dioxus, gpui, and other frontends. A first-pass node-kit base design now exists and is explicitly data-first.

# Verified State

- `jellyflow-runtime` already exposes `NodeSurfaceSlotDescriptor`, `PortViewDescriptor`, and `NodeKindViewDescriptor` as the semantic contract.
- `jellyflow-egui` already consumes that contract with `RichNodeRenderer`, `EguiNodeWidgetRenderer`, and `NodeInteractiveRegion`.
- `jellyflow-proof` and `templates/headless-adapter` already prove a second headless consumer boundary without egui widget ownership.
- Existing plans already defer a shared UI crate until a second adapter proves real reuse pressure.
- The repository is currently safe for fearless refactors that delete egui-specific leftovers once the adapter contract is explicit.

# Open Threads

- Whether the next durable artifact should be a new ADR for "adapter/node-kit boundary" or a plan-only refinement built on ADR 0008.
- How to split reusable node kits by product family: workflow, Dify-like automation, ERD/table, mind map, knowledge board.
- Which egui-specific renderer code should be reduced to adapter-local glue versus deleted outright.
- What the first manifest schema and semantic recipe data types should look like in `jellyflow-runtime::schema`.

# Next Action

Turn the node-kit design into concrete manifests, semantic recipe shapes, and first kit fixtures, then start deleting egui-only glue that no longer earns its keep.

# Citations

- [Current State](../current-state.md)
- [ADR 0008](../../adr/0008-semantic-surface-and-framework-adapter-boundary.md)
- [Semantic surface plan](../../../plans/2026-06-19-001-feat-semantic-surface-adapter-plan.md)
- [Second-adapter plan](../../../plans/2026-06-19-002-feat-second-adapter-semantic-surface-contract-plan.md)
- [jellyflow-runtime schema](../../../../crates/jellyflow-runtime/src/schema/types.rs)
- [jellyflow-egui renderer](../../../../crates/jellyflow-egui/src/renderer.rs)
- [jellyflow-proof crate](../../../../crates/jellyflow-proof/src/lib.rs)
