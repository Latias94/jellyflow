# JSR-015 Fret-core Dependency Detachment

Status: Complete
Last updated: 2026-05-30

## Outcome

`jellyflow-core` and `jellyflow-runtime` no longer depend on `fret-core`.

The headless Jellyflow crates now own or directly depend on the small value-type vocabulary they
need:

- `jellyflow-core` owns `NodeGraphModifiers` for headless modifier-policy matching.
- `jellyflow-runtime` depends directly on `keyboard-types` for serialized key-code contracts.
- `jellyflow-runtime` fit-view rect helpers use Jellyflow `CanvasRect` instead of `fret_core::Rect`.
- `fret-node` keeps Fret-specific conversions at the adapter boundary.

This makes the next external smoke meaningful: a consumer should be able to path-depend on
`jellyflow-core` and `jellyflow-runtime` without also depending on Fret crates.

## Code Changes

### `jellyflow-core`

- Removed `fret-core` from `ecosystem/jellyflow-core/Cargo.toml`.
- Added `NodeGraphModifiers` in `ecosystem/jellyflow-core/src/interaction/mod.rs`.
- Updated `NodeGraphModifierKey::is_pressed` to accept `NodeGraphModifiers`.
- Re-exported `NodeGraphModifiers` from `ecosystem/jellyflow-core/src/lib.rs`.
- Strengthened the manifest source-policy test to forbid `fret-core`.

### `jellyflow-runtime`

- Removed `fret-core` from `ecosystem/jellyflow-runtime/Cargo.toml`.
- Added direct `keyboard-types` dependency.
- Changed `NodeGraphKeyCode` and `NodeGraphDeleteKey::matches` to use `keyboard_types::Code`.
- Changed `compute_fit_view_target_for_canvas_rect` to accept Jellyflow `CanvasRect`.
- Strengthened the manifest source-policy test to forbid `fret-core`.

### `fret-node`

- Added adapter conversion from `fret_core::Modifiers` to `NodeGraphModifiers` in the declarative
  pointer-down path.
- Added `canvas_rect_from_fret_rect` in the UI viewport helper.
- Updated controller, binding, and view-reducer fit-view call sites to convert Fret rects at the
  adapter boundary.

## Dependency Evidence

`cargo tree -p jellyflow-core --depth 2` now shows only:

- `serde`
- `serde_json`
- `thiserror`
- `uuid`

`cargo tree -p jellyflow-runtime --depth 2` now shows:

- `jellyflow-core`
- `keyboard-types`
- `serde`
- `serde_json`
- `thiserror`
- `uuid`

No Fret crate appears in either headless dependency tree.

## Compatibility Notes

- `fret_core::KeyCode` is a re-export of `keyboard_types::Code`, so `fret-node` key matching keeps
  source compatibility while Jellyflow stops depending on Fret.
- `compute_fit_view_target_for_canvas_rect` is a Jellyflow runtime API and now takes
  `CanvasRect`. Fret-specific `Rect` conversion belongs in `fret-node`.
- `fret-node` still depends on `fret-core`, as intended for the Fret adapter.

## Fresh Gate Evidence

- `cargo check -p jellyflow-core`: passed.
- `cargo check -p jellyflow-runtime`: passed.
- `cargo check -p fret-node --no-default-features --features headless --tests`: passed.
- `cargo check -p fret-node --all-features --tests`: passed.
- `cargo nextest run -p jellyflow-core`: passed with 48 tests.
- `cargo nextest run -p jellyflow-runtime`: passed with 67 tests.
- `cargo nextest run -p fret-node --no-default-features`: passed with 24 tests.
- `cargo nextest run -p fret-node --all-features`: passed with 371 tests.
- `cargo clippy -p jellyflow-core --all-targets -- -D warnings`: passed.
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
- `cargo tree -p jellyflow-core --depth 2`: passed; no Fret crates in the tree.
- `cargo tree -p jellyflow-runtime --depth 2`: passed; no Fret crates in the tree.
- `python3 tools/check_layering.py`: passed.
- `cargo fmt --check`: passed.
