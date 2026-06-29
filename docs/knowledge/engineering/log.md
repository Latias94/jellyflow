# Engineering Memory Update Log

## 2026-06-19
* **Initialization**: Created engineering wiki memory bundle.
* **Decision capture**: Recorded the semantic-surface vs framework-adapter direction for Jellyflow UI.
* **ADR capture**: Added ADR 0008 to formalize the semantic surface boundary.
* **Plan capture**: Added a follow-up plan for semantic slots, rich node rendering, and selection UX.
* **Implementation slice**: Added runtime node surface slot descriptors, egui field-row slot rendering, decision-card rich rows, and partial-intersection box selection defaults.
* **Verification capture**: Recorded focused runtime/egui nextest checks, workspace check, public-surface checks, and gallery snapshot generation for automation-builder and ERD.
* **Second slice**: Extended egui semantic slots to badge, nested-region, and action-row rendering, with vertical action chips and larger sample node defaults.
* **Second verification**: Re-ran `cargo nextest run -p jellyflow-egui --no-fail-fast` and `cargo run -p jellyflow-egui --example gallery_snapshot`, then reviewed updated automation-builder and ERD snapshots.
* **Slot boundary refinement**: Treat `slot` as the data lookup path and keep `anchor` for adapter-local placement and port binding; legacy `field.*` rows still fall back to key tails.
* **Second-adapter proof**: Added a lightweight `jellyflow-proof` workspace crate that demonstrates a second adapter boundary without introducing a shared UI layer.
* **Session closeout**: Marked the second-adapter contract plan complete in workspace memory; the next useful track is adapter conformance around selection, hit regions, low-zoom degradation, and custom node UI semantics.
* **Verification closeout**: Confirmed the proof crate, runtime schema, and egui semantic-surface contract with `cargo fmt --all --check`, `cargo nextest run -p jellyflow-proof -p jellyflow-runtime -p jellyflow-egui --lib`, `cargo test -p jellyflow-proof`, `cargo test -p jellyflow-proof --example adapter_smoke`, and the targeted egui semantic-slot test.
* **Strategy clarification**: Confirmed the headless semantic-surface direction is correct; the next missing layer is an adapter/node-kit boundary for egui, Dioxus, gpui, and future frontends, not a shared widget crate.
* **Planning note**: Keep ADR 0008 as the durable architectural boundary for now; add a new ADR only if the adapter/node-kit boundary needs to be frozen after the next plan becomes concrete.
* **Node-kit design**: Defined node kits as versioned packages of semantic node families, surface recipes, default data, and fixtures, with adapters mapping those recipes to egui, Dioxus, or gpui locally.
* **Plan formalization**: Turned the node-kit boundary into `docs/plans/2026-06-19-003-feat-adapter-node-kit-boundary-plan.md`, with runtime schema manifest work, first kit families, egui cleanup, proof/template coverage, and docs updates split into implementation units.
* **Goal setup**: Created a goal to execute the node-kit plan end-to-end so the plan can be used as the active implementation target.
* **Implementation slice**: Reused `NodeKitRegistry::builtin()` in egui samples, proof, and the headless adapter template; added builtin kit fixture coverage for workflow, ERD, and mind-map; and fixed registry resolution so canonical kinds win over alias collisions.
* **Verification capture**: Confirmed `cargo fmt --all`, `cargo test -p jellyflow-runtime --lib schema::tests::kit -- --nocapture`, `cargo test -p jellyflow-egui`, `cargo test -p jellyflow-proof`, `cargo test -p jellyflow-proof --example adapter_smoke`, and `cargo test --manifest-path templates/headless-adapter/Cargo.toml` all passed.
* **Session capture**: Added a fresh handoff note for the node-kit implementation slice so the current state can survive compaction.
* **gpui substrate check**: Reviewed the `repo-ref/open-gpui` fork and confirmed its `canvas` crate already splits document/model, paint frame, painter, and widget overlay concerns, so Jellyflow can start a gpui proof from the existing substrate instead of forking a new canvas layer immediately.
* **gpui next step**: Treat `open-gpui` as the likely host for the next proof, and only plan library changes if the Jellyflow proof exposes missing slot-aware nested content or other semantic hooks that the current canvas substrate cannot express cleanly.
* **gpui overlay polish**: Fixed the `canvas-jellyflow` proof so semantic node content is rendered for every node instead of only feeling alive on selection, and added zoom-aware slot reduction plus `min_w(0)`/`min_h(0)`-style shrink constraints so rich node content stops overflowing blue nodes.
* **gpui verification**: Rechecked the example with `cargo check -p open-gpui-canvas-jellyflow` and `cargo nextest run -p open-gpui-canvas-jellyflow`; both passed after the layout refinement.

## 2026-06-20
* **gpui proof refresh**: Reworked `repo-ref/open-gpui/examples/canvas-jellyflow` to consume Jellyflow semantic descriptors from `NodeRegistry`/`NodeKitRegistry::builtin().node_registry()`, kept `CanvasKindRegistry` focused on renderer policy, and verified the example with `cargo check -p open-gpui-canvas-jellyflow`.
* **gpui continuity**: Added a fresh session handoff for the gpui proof so the retained-view adapter evidence survives compaction.
* **Density contract fix**: Corrected the `node_surface_projection_uses_layout_hints_for_density_and_slot_limits` test to assert the actual `display_label()`/data-preview contract for semantic slots, and cleaned up unused imports in the `open-gpui` jellyflow example.
* **Verification**: Re-ran `cargo nextest run -p jellyflow-runtime --lib` successfully; `cargo check --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml` also passed, but the broader workspace still emits pre-existing macOS `cargo-clippy` check-cfg warnings.

## 2026-06-23
* **gpui proof slot cap**: Added an adapter-local height-based slot cap in `repo-ref/open-gpui/examples/canvas-jellyflow` so full-density runtime projections do not visually overfill fixed-height gpui node surfaces.
* **gpui example verification**: Re-ran `cargo fmt --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --check`, `RUSTFLAGS='-Awarnings' cargo test --quiet --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --bin open-gpui-canvas-jellyflow`, and `RUSTFLAGS='-Awarnings' cargo check --quiet --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml`.
* **open-gpui sync attempt**: Fetched `repo-ref/open-gpui` from `origin/main`; `git pull --ff-only` could not merge because existing local `crates/gpui_docking` edits would be overwritten, so no stash, restore, reset, or checkout was used.
* **open-gpui sync resolved**: After confirming the local docking edits were an obsolete upstream intermediate state, the user approved discarding only `crates/gpui_docking`; `git restore -- crates/gpui_docking` followed by `git pull --ff-only` fast-forwarded open-gpui to `8d018ce`.
* **gpui proof cleanup**: Removed the stale `semantic_overlay` cache from `examples/canvas-jellyflow` so semantic node content is derived only from `NodeRegistry`/`NodeKitRegistry` at render time; reran format, check, tests, and a short launch smoke.

## 2026-06-29
* **open-gpui proof commit**: Committed the GPUI Jellyflow semantic node surface proof as `ce14140 feat(canvas): prove jellyflow semantic node surfaces` in `repo-ref/open-gpui`; the fork is now clean and ahead of origin by one commit.
* **ADR capture**: Added ADR 0009 to freeze node kits as versioned semantic packages with adapter-local mapping, deferring any shared widget crate until multiple adapters prove real reuse pressure.
* **Main repo verification**: Re-ran the main gate after ADR and memory updates: `cargo fmt --all --check`, `cargo nextest run -p jellyflow-runtime -p jellyflow-egui -p jellyflow-proof --lib` (469 tests), `cargo test -p jellyflow-proof --example adapter_smoke`, and `cargo test --manifest-path templates/headless-adapter/Cargo.toml` all passed.
* **Projection contract fix**: Moved field-row value projection into runtime so `surface_slots_projection` reads `data.fields[slot]` before ordinary JSON paths; this keeps GPUI, proof, and future Dioxus adapters on the same value lookup rule as egui.
* **Verification refresh**: After the projection fix, re-ran `cargo fmt --all --check`, `cargo nextest run -p jellyflow-runtime -p jellyflow-egui -p jellyflow-proof --lib` (469 tests), `cargo test -p jellyflow-proof --example adapter_smoke`, `cargo test --manifest-path templates/headless-adapter/Cargo.toml`, and `RUSTFLAGS='-Awarnings' cargo test --quiet --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --bin open-gpui-canvas-jellyflow`; all passed.
* **Main repo commit**: Committed the node-kit/runtime/egui/proof/docs work as `feat(schema): add semantic node kit boundary` in the main Jellyflow repo.
