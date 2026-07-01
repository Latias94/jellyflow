---
type: "Current State"
title: "Current Engineering State"
description: "Short durable summary of the active engineering state."
tags: ["engineering-memory"]
timestamp: 2026-07-01T00:40:00+08:00
status: "complete"
---

# Current State

- Goal: Finish the Open GPUI authoring facade cleanup plan while preserving the headless semantic contract and adapter-local widget boundary.
- Branch: `feat/xyflow-product-surface`
- Last verified: 2026-07-01 Open GPUI authoring facade U4 focused gates passed: `cargo fmt --all -- --check`, `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml -- --check`, `git diff --check`, `git -C repo-ref/open-gpui diff --check`, `cargo nextest run -p jellyflow-open-gpui --no-fail-fast` (68 tests), `cargo check --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml`, and `cargo test --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --bin open-gpui-canvas-jellyflow -- --nocapture` (25 tests). Existing open-gpui macOS `check-cfg` / `unused_unsafe` warnings remain out of scope.
- Done: ADR 0003, ADR 0008, ADR 0009, follow-up plans, semantic slot schema, egui field-row slot rendering, decision-card rich rows, selection-mode regression tests, README/CHANGELOG updates, gallery visual review for automation-builder and ERD, runtime slot/anchor helpers, second-adapter proof crate, adapter conformance coverage for selection, geometry, viewport, and product fixtures, strategic confirmation that the seam should stay semantic rather than widget-based, a first-pass node-kit base design, builtin runtime node-kit manifests/fixtures, egui sample reuse of builtin kits, proof/template reuse of builtin kits, the canonical-kind-over-alias registry precedence fix, runtime field-row value projection that reads `data.fields[slot]` before ordinary JSON paths, the `open-gpui` canvas substrate check that confirmed gpui can start from the existing document / paint-model / overlay split, the `open-gpui` canvas-jellyflow proof refresh that now reads semantic descriptors from Jellyflow `NodeRegistry` while keeping `CanvasKindRegistry` on renderer policy only, the gpui overlay polish that made semantic node content always visible with zoom-aware slot reduction and tighter flex shrink constraints, the density/slot projection contract fix, a gpui example adapter-local slot-height cap so full-density semantic slots do not overfill fixed-height nodes, the removal of stale `semantic_overlay` canvas data so gpui proof has one semantic source of truth, the approved cleanup of obsolete local `gpui_docking` edits so `repo-ref/open-gpui` could fast-forward to `origin/main` at `8d018ce`, the main-repo node-kit boundary commit, commit `ce14140` in `repo-ref/open-gpui` for the gpui semantic node surface proof, the implementation-ready plan for node UI capability parity, runtime measurement/dynamic internals/geometry/connection/edge-route/chrome slices U1-U6, expanded U7 semantic slot recipes for Dify-style workflow, shader/blueprint, ERD, and knowledge canvas nodes, U8 egui shader graph and measured rich-node renderer hardening, U9 GPUI shader fixture and slot-anchor projection proof, U10 Dioxus-shaped component tree proof, U11 product-shaped example gallery, U12 authoring/checklist documentation, the 2026-06-30 review-fix slice: `NodeInternalsController`, egui current-frame remeasurement and dirty invalidation tests, shader typed-port hover/commit rejection through the same typed planner, GPUI shared component-layout measurement proof, and proof dynamic child remeasurement coverage, the Node UI Authoring Contracts U1-U8 implementation, the Open GPUI Mature Adapter U1-U9 slices through docs cleanup and product fixture regression gates in `jellyflow-open-gpui::testing`, the Open GPUI Layout-Pass Measurement plan and implementation, the Open GPUI Productized Authoring plan implementation, and the Open GPUI Authoring Facade Cleanup U1-U4 commits: adapter-local JSON binding (`8e8768e`), live-store control planning (`7394455`), semantic repeatable action dispatch (`c2ed930`), and renderer host facade plus scoped element ids (`e201f37`), with matching local `repo-ref/open-gpui` example commits through `96f6b4f`.
- In progress: U5 documentation and memory refresh for `docs/plans/2026-07-01-002-refactor-open-gpui-authoring-facade-plan.md`; final broad verification and review gates remain.
- Blocked: None.
- Next action: Finish U5 docs/memory, run the plan's final verification contract, review the diff against the Definition of Done, then commit only the documentation/memory refresh.

# Notes

- `cargo nextest run -p jellyflow-runtime -p jellyflow-egui -p jellyflow-proof --lib --no-fail-fast` now runs 529 tests across runtime, egui, and proof.
- Low-zoom visual confidence was checked through gallery snapshots; the automation builder now degrades node internals instead of overlapping Dify-style controls, while shader and ERD examples keep readable anchors and wires.
- Subagent research on xyflow, egui-snarl, Rete, Unreal Blueprint, Unity Shader Graph, Dify, and mind-map products reinforced the current boundary: headless owns semantic node kits, typed ports, slots, anchors, chrome, measurement, validation, and action/menu descriptors; adapters own widgets, focus, menus, layout measurement, and local component libraries. The Node UI Kit Component Contract is now captured as the next design layer; the remaining design gap is a formal adapter capability matrix, not a shared widget crate.
- The Node UI Authoring Contracts plan is implemented as semantic contracts, not a shared widget crate. GPUI now has a first-class adapter crate and a live `measured_element` consumer; capability reporting must remain coverage-gated so projection/partial reports do not imply full layout-pass measurement.
- The current plan intentionally narrows maturity work to open-gpui only. egui and Dioxus should not be expanded in this stage unless shared runtime compatibility requires it.
- Review gate outcome: local code review found and fixed one real issue in the GPUI example: repeated identical layout-pass facts originally incremented `measurement_revision`, which would make `NodeGraphStore::report_node_measurement` report changed every frame. The fix reuses the existing revision when geometry is unchanged and fresh, while dirty/missing or changed geometry still bumps revision. A regression test covers the no-refresh path.
- Open GPUI authoring facade cleanup refined ownership: `jellyflow-open-gpui` owns binding, live-store authoring planning, semantic repeatable action mapping, scoped element ids, renderer resolution/fallback, and generic host-context services; `canvas-jellyflow` owns concrete Open GPUI widgets, weak-entity dispatch, focus/popup behavior, demo defaults, visual layout, and refresh notifications.

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
- [Open GPUI Layout-Pass Measurement Plan](../../plans/2026-06-30-003-feat-open-gpui-layout-pass-measurement-plan.md)
- [Open GPUI Productized Authoring Plan](../../plans/2026-07-01-001-feat-open-gpui-productized-authoring-plan.md)
- [Open GPUI Authoring Facade Cleanup Plan](../../plans/2026-07-01-002-refactor-open-gpui-authoring-facade-plan.md)
- [Open GPUI Productized Authoring Findings](subagents/2026-07-01-open-gpui-productized-authoring-findings.md)
- [jellyflow-egui renderer](../../../crates/jellyflow-egui/src/renderer.rs)
- [jellyflow-proof crate](../../../crates/jellyflow-proof/src/lib.rs)
- [open-gpui pull handoff](sessions/2026-06-23-open-gpui-pull-blocked-by-existing-local-changes.md)
