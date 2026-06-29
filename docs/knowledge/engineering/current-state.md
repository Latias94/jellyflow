---
type: "Current State"
title: "Current Engineering State"
description: "Short durable summary of the active engineering state."
tags: ["engineering-memory"]
timestamp: 2026-06-29T06:38:24Z
status: "active"
---

# Current State

- Goal: Complete the Jellyflow adapter/node-kit boundary refactor from the node-kit plan, with runtime schema manifests, first kit families, egui glue cleanup, proof/template coverage, and docs/memory updates.
- Branch: `feat/xyflow-product-surface`
- Last verified: Main Jellyflow gate passed with `cargo fmt --all --check`, `cargo nextest run -p jellyflow-runtime -p jellyflow-egui -p jellyflow-proof --lib` (469 tests), `cargo test -p jellyflow-proof --example adapter_smoke`, and `cargo test --manifest-path templates/headless-adapter/Cargo.toml`; for `repo-ref/open-gpui/examples/canvas-jellyflow`, `RUSTFLAGS='-Awarnings' cargo test --quiet --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --bin open-gpui-canvas-jellyflow`, the prior format/check gate, and a short `cargo run` launch smoke passed.
- Done: ADR 0003, ADR 0008, ADR 0009, follow-up plans, semantic slot schema, egui field-row slot rendering, decision-card rich rows, selection-mode regression tests, README/CHANGELOG updates, gallery visual review for automation-builder and ERD, runtime slot/anchor helpers, second-adapter proof crate, adapter conformance coverage for selection, geometry, viewport, and product fixtures, strategic confirmation that the seam should stay semantic rather than widget-based, a first-pass node-kit base design, builtin runtime node-kit manifests/fixtures, egui sample reuse of builtin kits, proof/template reuse of builtin kits, the canonical-kind-over-alias registry precedence fix, runtime field-row value projection that reads `data.fields[slot]` before ordinary JSON paths, the `open-gpui` canvas substrate check that confirmed gpui can start from the existing document / paint-model / overlay split, the `open-gpui` canvas-jellyflow proof refresh that now reads semantic descriptors from Jellyflow `NodeRegistry` while keeping `CanvasKindRegistry` on renderer policy only, the gpui overlay polish that made semantic node content always visible with zoom-aware slot reduction and tighter flex shrink constraints, the density/slot projection contract fix, a gpui example adapter-local slot-height cap so full-density semantic slots do not overfill fixed-height nodes, the removal of stale `semantic_overlay` canvas data so gpui proof has one semantic source of truth, the approved cleanup of obsolete local `gpui_docking` edits so `repo-ref/open-gpui` could fast-forward to `origin/main` at `8d018ce`, the main-repo node-kit boundary commit, and commit `ce14140` in `repo-ref/open-gpui` for the gpui semantic node surface proof.
- In progress: None.
- Blocked: None.
- Next action: Push or open review for the main Jellyflow node-kit boundary commit and the separate `repo-ref/open-gpui` proof commit, then start the next adapter proof slice. The most useful next technical step is a Dioxus proof that consumes `NodeKitRegistry::builtin()` without adding a shared widget crate.

# Citations

- [ADR 0003](../../adr/0003-headless-adapter-testing-and-renderer-boundary.md)
- [ADR 0005](../../adr/0005-layout-engine-extension-boundary.md)
- [ADR 0007](../../adr/0007-knowledge-canvas-foundations.md)
- [ADR 0008](../../adr/0008-semantic-surface-and-framework-adapter-boundary.md)
- [ADR 0009](../../adr/0009-node-kit-and-adapter-local-mapping-boundary.md)
- [jellyflow-egui renderer](../../../crates/jellyflow-egui/src/renderer.rs)
- [jellyflow-proof crate](../../../crates/jellyflow-proof/src/lib.rs)
- [open-gpui pull handoff](sessions/2026-06-23-open-gpui-pull-blocked-by-existing-local-changes.md)
