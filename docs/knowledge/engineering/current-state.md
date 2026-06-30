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
- Last verified: 2026-06-30 Open GPUI Mature Adapter final gate passed: `cargo fmt --all -- --check`, `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -- --check`, main/open-gpui `git diff --check`, `cargo nextest run -p jellyflow-open-gpui --no-fail-fast` (25 tests), `cargo nextest run -p jellyflow-runtime -p jellyflow-egui -p jellyflow-proof --lib --no-fail-fast` (529 tests), runtime public-surface tests (3 tests), proof integration tests (3 tests), headless adapter template tests (22 tests), `cargo check -p jellyflow-egui --examples`, `cargo check --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml`, GPUI canvas-jellyflow tests (13 tests), `cargo test --manifest-path repo-ref/open-gpui/crates/gpui/Cargo.toml measured_element_reports_nested_layout_pass_bounds -- --nocapture` (1 test), and a short GPUI launch smoke.
- Done: ADR 0003, ADR 0008, ADR 0009, follow-up plans, semantic slot schema, egui field-row slot rendering, decision-card rich rows, selection-mode regression tests, README/CHANGELOG updates, gallery visual review for automation-builder and ERD, runtime slot/anchor helpers, second-adapter proof crate, adapter conformance coverage for selection, geometry, viewport, and product fixtures, strategic confirmation that the seam should stay semantic rather than widget-based, a first-pass node-kit base design, builtin runtime node-kit manifests/fixtures, egui sample reuse of builtin kits, proof/template reuse of builtin kits, the canonical-kind-over-alias registry precedence fix, runtime field-row value projection that reads `data.fields[slot]` before ordinary JSON paths, the `open-gpui` canvas substrate check that confirmed gpui can start from the existing document / paint-model / overlay split, the `open-gpui` canvas-jellyflow proof refresh that now reads semantic descriptors from Jellyflow `NodeRegistry` while keeping `CanvasKindRegistry` on renderer policy only, the gpui overlay polish that made semantic node content always visible with zoom-aware slot reduction and tighter flex shrink constraints, the density/slot projection contract fix, a gpui example adapter-local slot-height cap so full-density semantic slots do not overfill fixed-height nodes, the removal of stale `semantic_overlay` canvas data so gpui proof has one semantic source of truth, the approved cleanup of obsolete local `gpui_docking` edits so `repo-ref/open-gpui` could fast-forward to `origin/main` at `8d018ce`, the main-repo node-kit boundary commit, commit `ce14140` in `repo-ref/open-gpui` for the gpui semantic node surface proof, the implementation-ready plan for node UI capability parity, runtime measurement/dynamic internals/geometry/connection/edge-route/chrome slices U1-U6, expanded U7 semantic slot recipes for Dify-style workflow, shader/blueprint, ERD, and knowledge canvas nodes, U8 egui shader graph and measured rich-node renderer hardening, U9 GPUI shader fixture and slot-anchor projection proof, U10 Dioxus-shaped component tree proof, U11 product-shaped example gallery, U12 authoring/checklist documentation, the 2026-06-30 review-fix slice: `NodeInternalsController`, egui current-frame remeasurement and dirty invalidation tests, shader typed-port hover/commit rejection through the same typed planner, GPUI shared component-layout measurement proof, and proof dynamic child remeasurement coverage, the Node UI Authoring Contracts U1-U8 implementation, and the Open GPUI Mature Adapter U1-U9 slices through docs cleanup and product fixture regression gates in `jellyflow-open-gpui::testing`.
- In progress: None for the Open GPUI Mature Adapter plan. The adapter crate owns reusable projection, controls, repeatables, menus/actions, inspector, and product fixture gates; `canvas-jellyflow` consumes those helpers and should not regain adapter ownership.
- Blocked: None.
- Next action: Start the next plan only if the project wants to promote GPUI from projection fallback to full retained-view geometry by wiring the measured-element hook into live `canvas-jellyflow` node component rendering and collecting real layout-pass bounds in the adapter.

# Notes

- `cargo nextest run -p jellyflow-runtime -p jellyflow-egui -p jellyflow-proof --lib --no-fail-fast` now runs 529 tests across runtime, egui, and proof.
- Low-zoom visual confidence was checked through gallery snapshots; the automation builder now degrades node internals instead of overlapping Dify-style controls, while shader and ERD examples keep readable anchors and wires.
- Subagent research on xyflow, egui-snarl, Rete, Unreal Blueprint, Unity Shader Graph, Dify, and mind-map products reinforced the current boundary: headless owns semantic node kits, typed ports, slots, anchors, chrome, measurement, validation, and action/menu descriptors; adapters own widgets, focus, menus, layout measurement, and local component libraries. The Node UI Kit Component Contract is now captured as the next design layer; the remaining design gap is a formal adapter capability matrix, not a shared widget crate.
- The Node UI Authoring Contracts plan is implemented as semantic contracts, not a shared widget crate. GPUI now has a first-class adapter crate, but capability reporting must remain honest: projection fixture gates do not imply full layout-pass measurement unless real open-gpui element bounds are the source.
- The current plan intentionally narrows maturity work to open-gpui only. egui and Dioxus should not be expanded in this stage unless shared runtime compatibility requires it.
- Review gate outcome: local plan review found runtime/core/layout remain toolkit-free, `jellyflow-open-gpui` is the only Jellyflow crate that depends on the local Open GPUI fork, and capability reporting keeps `ProjectionFallback` separate from `LayoutPass`. A read-only auxiliary review agent was attempted but interrupted after repeated timeouts, so the final recorded review evidence is the local diff/plan audit plus the verification gates above.

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
