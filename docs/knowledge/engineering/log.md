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
