---
type: "Session Handoff"
title: "gpui proof semantic projection and layout constraints"
description: "Handoff for the open-gpui canvas proof follow-up that keeps semantic node content visible for all nodes while constraining rich overlays with zoom-aware slot reduction and flex shrink rules."
tags: ["engineering-memory", "jellyflow", "gpui", "proof", "overlay", "semantic-surface"]
timestamp: 2026-06-20T06:15:00Z
status: "active"
---

# Summary

The `repo-ref/open-gpui/examples/canvas-jellyflow` proof was refined after a visual review revealed two issues: rich node content felt like it only appeared when nodes were selected, and the blue node content could overflow its bounds. The example now treats semantic descriptors as the always-on source of node content and uses zoom-aware slot reduction plus flex shrink constraints to keep the overlay inside the node shell.

# Verified State

- The example still projects Jellyflow runtime graph data into `open-gpui-canvas`.
- `NodeKitRegistry::builtin().node_registry()` remains the semantic source for node surface content.
- Semantic content now renders for every node; selection only changes emphasis.
- Overlay content is zoom-aware: low zoom shows fewer slots, higher zoom shows more.
- The node surface cards use `min_w(0)`/`min_h(0)`-style shrink constraints so truncate and line-clamp can actually work inside flex layouts.
- Verified with `cargo check -p open-gpui-canvas-jellyflow` and `cargo nextest run -p open-gpui-canvas-jellyflow`.

# Open Threads

- Whether the next gpui slice should expose a reusable semantic overlay helper instead of keeping the projection local to the example.
- Whether `open-gpui` needs a first-class layout helper for semantic node shells or the example-level pattern is enough for now.

# Next Action

Use the current proof as visual evidence for the retained-view adapter path, then only extract a reusable seam if the next adapter or kit slice exposes repeated overlay projection logic.

# Citations

- [open-gpui canvas-jellyflow example](../../../../repo-ref/open-gpui/examples/canvas-jellyflow/src/main.rs)
- [Current State](../current-state.md)
- [Engineering Log](../log.md)
