---
type: "Current State"
title: "Current Engineering State"
description: "Short durable summary of the active engineering state."
tags: ["engineering-memory"]
timestamp: 2026-06-30T02:33:07+08:00
status: "complete"
---

# Current State

- Goal: Execute the node UI capability parity plan so Jellyflow reaches egui-snarl/XYFlow-class custom node UI strength while preserving the headless semantic-surface and node-kit boundary.
- Branch: `feat/xyflow-product-surface`
- Last verified: 2026-06-30 broad review-fix verification passed: `cargo fmt --all -- --check`, `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --check`, `git diff --check`, `git -C repo-ref/open-gpui diff --check`, `cargo nextest run -p jellyflow-runtime -p jellyflow-egui -p jellyflow-proof --lib --no-fail-fast`, `cargo test -p jellyflow-proof --test proof -- --nocapture`, `cargo test --manifest-path templates/headless-adapter/Cargo.toml`, `cargo check -p jellyflow-egui --examples`, `RUSTFLAGS='-Awarnings' cargo check --quiet --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml`, and `RUSTFLAGS='-Awarnings' cargo test --quiet --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --bin open-gpui-canvas-jellyflow`.
- Done: ADR 0003, ADR 0008, ADR 0009, follow-up plans, semantic slot schema, egui field-row slot rendering, decision-card rich rows, selection-mode regression tests, README/CHANGELOG updates, gallery visual review for automation-builder and ERD, runtime slot/anchor helpers, second-adapter proof crate, adapter conformance coverage for selection, geometry, viewport, and product fixtures, strategic confirmation that the seam should stay semantic rather than widget-based, a first-pass node-kit base design, builtin runtime node-kit manifests/fixtures, egui sample reuse of builtin kits, proof/template reuse of builtin kits, the canonical-kind-over-alias registry precedence fix, runtime field-row value projection that reads `data.fields[slot]` before ordinary JSON paths, the `open-gpui` canvas substrate check that confirmed gpui can start from the existing document / paint-model / overlay split, the `open-gpui` canvas-jellyflow proof refresh that now reads semantic descriptors from Jellyflow `NodeRegistry` while keeping `CanvasKindRegistry` on renderer policy only, the gpui overlay polish that made semantic node content always visible with zoom-aware slot reduction and tighter flex shrink constraints, the density/slot projection contract fix, a gpui example adapter-local slot-height cap so full-density semantic slots do not overfill fixed-height nodes, the removal of stale `semantic_overlay` canvas data so gpui proof has one semantic source of truth, the approved cleanup of obsolete local `gpui_docking` edits so `repo-ref/open-gpui` could fast-forward to `origin/main` at `8d018ce`, the main-repo node-kit boundary commit, commit `ce14140` in `repo-ref/open-gpui` for the gpui semantic node surface proof, the implementation-ready plan for node UI capability parity, runtime measurement/dynamic internals/geometry/connection/edge-route/chrome slices U1-U6, expanded U7 semantic slot recipes for Dify-style workflow, shader/blueprint, ERD, and knowledge canvas nodes, U8 egui shader graph and measured rich-node renderer hardening, U9 GPUI shader fixture and slot-anchor projection proof, U10 Dioxus-shaped component tree proof, U11 product-shaped example gallery, U12 authoring/checklist documentation, and the 2026-06-30 review-fix slice: `NodeInternalsController`, egui current-frame remeasurement and dirty invalidation tests, shader typed-port hover/commit rejection through the same typed planner, GPUI shared component-layout measurement proof, and proof dynamic child remeasurement coverage.
- In progress: Commit split review for the review-fix slice.
- Blocked: None.
- Next action: Ask before staging and committing. Suggested split is still runtime/core contracts, egui examples/renderer, proof/template/docs, and GPUI proof, with the 2026-06-30 review fixes either folded into the relevant split or committed as a focused follow-up.

# Notes

- `cargo nextest run -p jellyflow-runtime --lib` now completes normally with 454 passing tests; the broader runtime/egui/proof lib gate runs 505 tests.
- Low-zoom visual confidence was checked through gallery snapshots; the automation builder now degrades node internals instead of overlapping Dify-style controls, while shader and ERD examples keep readable anchors and wires.
- Subagent research on xyflow, egui-snarl, Rete, Unreal Blueprint, Unity Shader Graph, Dify, and mind-map products reinforced the current boundary: headless owns semantic node kits, typed ports, slots, anchors, chrome, measurement, validation, and action/menu descriptors; adapters own widgets, focus, menus, layout measurement, and local component libraries. The Node UI Kit Component Contract is now captured as the next design layer; the remaining design gap is a formal adapter capability matrix, not a shared widget crate.

# Citations

- [ADR 0003](../../adr/0003-headless-adapter-testing-and-renderer-boundary.md)
- [ADR 0005](../../adr/0005-layout-engine-extension-boundary.md)
- [ADR 0007](../../adr/0007-knowledge-canvas-foundations.md)
- [ADR 0008](../../adr/0008-semantic-surface-and-framework-adapter-boundary.md)
- [ADR 0009](../../adr/0009-node-kit-and-adapter-local-mapping-boundary.md)
- [Node UI Kit Component Contract](decisions/node-ui-kit-component-contract.md)
- [Node UI Capability Parity Plan](../../plans/2026-06-29-001-feat-node-ui-capability-parity-plan.md)
- [jellyflow-egui renderer](../../../crates/jellyflow-egui/src/renderer.rs)
- [jellyflow-proof crate](../../../crates/jellyflow-proof/src/lib.rs)
- [open-gpui pull handoff](sessions/2026-06-23-open-gpui-pull-blocked-by-existing-local-changes.md)
