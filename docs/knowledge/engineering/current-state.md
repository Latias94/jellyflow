---
type: "Current State"
title: "Current Engineering State"
description: "Short durable summary of the active engineering state."
tags: ["engineering-memory"]
timestamp: 2026-06-19T05:45:17Z
status: "active"
---

# Current State

- Goal: Clarify the Jellyflow semantic surface and adapter boundary for egui and future frontends.
- Branch: `feat/xyflow-product-surface`
- Last verified: Added runtime semantic surface slot descriptors, wired egui field-row regions to those slots, changed default box selection to partial intersection, updated docs/examples, and ran focused runtime/egui checks plus workspace check and gallery snapshot generation.
- Done: ADR 0008, follow-up plan, semantic slot schema, egui field-row slot rendering, decision-card rich rows, selection-mode regression tests, README/CHANGELOG updates, and gallery visual review for automation-builder and ERD.
- In progress: None for the first semantic-surface slice.
- Blocked: None.
- Next action: Commit the first slice, then decide whether to expand action/badge/nested-region examples or add a second adapter/prototype conformance check.

# Citations

- [ADR 0003](../../adr/0003-headless-adapter-testing-and-renderer-boundary.md)
- [ADR 0005](../../adr/0005-layout-engine-extension-boundary.md)
- [ADR 0007](../../adr/0007-knowledge-canvas-foundations.md)
- [jellyflow-egui renderer](../../../crates/jellyflow-egui/src/renderer.rs)
