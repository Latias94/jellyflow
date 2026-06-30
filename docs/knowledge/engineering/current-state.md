---
type: "Current State"
title: "Current Engineering State"
description: "Short durable summary of the active engineering state."
tags: ["engineering-memory"]
timestamp: 2026-06-30T22:00:00+08:00
status: "complete"
---

# Current State

- Goal: Prepare the next fearless-refactor stage: make `repo-ref/open-gpui` Jellyflow's first mature retained UI adapter while preserving the headless semantic contract and adapter-local widget boundary.
- Branch: `feat/xyflow-product-surface`
- Last verified: 2026-06-30 Node UI Authoring Contracts verification passed: `cargo fmt --all -- --check`, `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --check`, `git diff --check`, `git -C repo-ref/open-gpui diff --check`, `cargo nextest run -p jellyflow-runtime -p jellyflow-egui -p jellyflow-proof --lib --no-fail-fast` (529 tests), `cargo test -p jellyflow-runtime --test public_surface -- --nocapture`, `cargo test -p jellyflow-proof --test proof -- --nocapture`, `cargo test --manifest-path templates/headless-adapter/Cargo.toml`, `cargo check -p jellyflow-egui --examples`, `cargo run -p jellyflow-egui --example gallery_snapshot -- target/jellyflow-egui-gallery`, `RUSTFLAGS='-Awarnings' cargo check --quiet --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml`, and `RUSTFLAGS='-Awarnings' cargo test --quiet --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --bin open-gpui-canvas-jellyflow`.
- Done: ADR 0003, ADR 0008, ADR 0009, follow-up plans, semantic slot schema, egui field-row slot rendering, decision-card rich rows, selection-mode regression tests, README/CHANGELOG updates, gallery visual review for automation-builder and ERD, runtime slot/anchor helpers, second-adapter proof crate, adapter conformance coverage for selection, geometry, viewport, and product fixtures, strategic confirmation that the seam should stay semantic rather than widget-based, a first-pass node-kit base design, builtin runtime node-kit manifests/fixtures, egui sample reuse of builtin kits, proof/template reuse of builtin kits, the canonical-kind-over-alias registry precedence fix, runtime field-row value projection that reads `data.fields[slot]` before ordinary JSON paths, the `open-gpui` canvas substrate check that confirmed gpui can start from the existing document / paint-model / overlay split, the `open-gpui` canvas-jellyflow proof refresh that now reads semantic descriptors from Jellyflow `NodeRegistry` while keeping `CanvasKindRegistry` on renderer policy only, the gpui overlay polish that made semantic node content always visible with zoom-aware slot reduction and tighter flex shrink constraints, the density/slot projection contract fix, a gpui example adapter-local slot-height cap so full-density semantic slots do not overfill fixed-height nodes, the removal of stale `semantic_overlay` canvas data so gpui proof has one semantic source of truth, the approved cleanup of obsolete local `gpui_docking` edits so `repo-ref/open-gpui` could fast-forward to `origin/main` at `8d018ce`, the main-repo node-kit boundary commit, commit `ce14140` in `repo-ref/open-gpui` for the gpui semantic node surface proof, the implementation-ready plan for node UI capability parity, runtime measurement/dynamic internals/geometry/connection/edge-route/chrome slices U1-U6, expanded U7 semantic slot recipes for Dify-style workflow, shader/blueprint, ERD, and knowledge canvas nodes, U8 egui shader graph and measured rich-node renderer hardening, U9 GPUI shader fixture and slot-anchor projection proof, U10 Dioxus-shaped component tree proof, U11 product-shaped example gallery, U12 authoring/checklist documentation, the 2026-06-30 review-fix slice: `NodeInternalsController`, egui current-frame remeasurement and dirty invalidation tests, shader typed-port hover/commit rejection through the same typed planner, GPUI shared component-layout measurement proof, and proof dynamic child remeasurement coverage, and the Node UI Authoring Contracts U1-U8 implementation: capability requirements/gaps, Field/Control Descriptor, repeatable collections with stable numeric ids, action/menu/inspector/blackboard descriptors, builtin product fixtures, egui authoring controls/repeatables/dropped-wire/inspector/geometry, GPUI projection proof, template adapter conformance, and visual regression docs.
- In progress: Next-stage plan `docs/plans/2026-06-30-002-feat-open-gpui-mature-adapter-plan.md` has been created to graduate GPUI from projection proof to mature adapter through a new `jellyflow-open-gpui` crate, real open-gpui element bounds, descriptor-driven controls, repeatables, menus, dropped-wire actions, inspector, and regression gates.
- Blocked: None.
- Next action: If the user approves execution, run the Open GPUI Mature Adapter plan via `ce-work` or goal mode. Treat `repo-ref/open-gpui` as a separate repo on local `main`, currently ahead of origin by the previously committed GPUI proof work.

# Notes

- `cargo nextest run -p jellyflow-runtime -p jellyflow-egui -p jellyflow-proof --lib --no-fail-fast` now runs 529 tests across runtime, egui, and proof.
- Low-zoom visual confidence was checked through gallery snapshots; the automation builder now degrades node internals instead of overlapping Dify-style controls, while shader and ERD examples keep readable anchors and wires.
- Subagent research on xyflow, egui-snarl, Rete, Unreal Blueprint, Unity Shader Graph, Dify, and mind-map products reinforced the current boundary: headless owns semantic node kits, typed ports, slots, anchors, chrome, measurement, validation, and action/menu descriptors; adapters own widgets, focus, menus, layout measurement, and local component libraries. The Node UI Kit Component Contract is now captured as the next design layer; the remaining design gap is a formal adapter capability matrix, not a shared widget crate.
- The Node UI Authoring Contracts plan is implemented as semantic contracts, not a shared widget crate. GPUI remains an honest `ProjectionFallback` proof until `open-gpui` exposes stable layout-pass element bounds for adapter-local component measurement.
- The next plan intentionally narrows maturity work to open-gpui only. egui and Dioxus should not be expanded in this stage unless shared runtime compatibility requires it.

# Citations

- [ADR 0003](../../adr/0003-headless-adapter-testing-and-renderer-boundary.md)
- [ADR 0005](../../adr/0005-layout-engine-extension-boundary.md)
- [ADR 0007](../../adr/0007-knowledge-canvas-foundations.md)
- [ADR 0008](../../adr/0008-semantic-surface-and-framework-adapter-boundary.md)
- [ADR 0009](../../adr/0009-node-kit-and-adapter-local-mapping-boundary.md)
- [Node UI Kit Component Contract](decisions/node-ui-kit-component-contract.md)
- [Node UI Capability Parity Plan](../../plans/2026-06-29-001-feat-node-ui-capability-parity-plan.md)
- [Node UI Authoring Contracts Plan](../../plans/2026-06-30-001-feat-node-ui-authoring-contracts-plan.md)
- [Open GPUI Mature Adapter Plan](../../plans/2026-06-30-002-feat-open-gpui-mature-adapter-plan.md)
- [jellyflow-egui renderer](../../../crates/jellyflow-egui/src/renderer.rs)
- [jellyflow-proof crate](../../../crates/jellyflow-proof/src/lib.rs)
- [open-gpui pull handoff](sessions/2026-06-23-open-gpui-pull-blocked-by-existing-local-changes.md)
