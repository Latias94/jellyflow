---
type: "Decision"
title: "Semantic Surface Belongs In The Headless Graph"
description: "Use renderer-neutral semantic tags and regions in core; keep concrete widgets inside adapters."
tags: ["engineering-memory", "jellyflow", "adapter", "ui-surface"]
timestamp: 2026-06-19T05:45:17Z
status: "active"
---

# Decision

Jellyflow should expose a **semantic surface** instead of framework-specific UI objects. Core
should define node and port meaning, layout intent, and interactive regions. Adapters should turn
those semantics into egui, DOM, canvas, or native widgets.

# Context

The repository already treats the engine as headless and adapter-driven. Existing ADRs keep
`jellyflow-core` and `jellyflow-runtime` renderer-free, keep layout extensible, and treat
rendering/query concerns as adapter-owned. ADR 0008 now makes the semantic-surface boundary
explicit. The current egui adapter already has:

- renderer-neutral metadata such as `renderer_key` and `PortViewDescriptor`;
- node-local hit and layout regions via `NodeInteractiveRegion`;
- adapter-owned render traits such as `RichNodeRenderer` and `EguiNodeWidgetRenderer`.

That means the remaining decision is not whether adapters exist, but how much UI shape should live
in the headless surface.

# Alternatives

1. Put framework UI objects in core.
   - Rejected. That would lock Jellyflow to one frontend model and make future adapters shallow.
2. Keep only raw text labels in core.
   - Rejected. That is too weak for complex nodes, field rows, badges, nested regions, and
     framework-specific rendering.
3. Expose semantic slots and regions in core, let adapters render them.
   - Chosen. This keeps the seam small and preserves portability.

# Consequences

- The headless crates stay portable across egui, web, native, and headless consumers.
- Complex nodes can still support rich internal UI without storing frontend widgets in the graph.
- Future adapters only need to map semantic slots to their local rendering toolkit.
- Documentation and examples should describe node surface in semantic terms, not framework types.

# Citations

- [ADR 0003](../../../adr/0003-headless-adapter-testing-and-renderer-boundary.md)
- [ADR 0005](../../../adr/0005-layout-engine-extension-boundary.md)
- [ADR 0007](../../../adr/0007-knowledge-canvas-foundations.md)
- [ADR 0008](../../../adr/0008-semantic-surface-and-framework-adapter-boundary.md)
- [jellyflow-egui renderer](../../../../crates/jellyflow-egui/src/renderer.rs)
