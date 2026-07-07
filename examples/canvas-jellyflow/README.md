# Open GPUI Canvas Jellyflow Example

This package is Jellyflow's concrete Open GPUI product gallery.

It intentionally lives outside the Jellyflow root workspace so the default
Jellyflow checks do not pull the Open GPUI native stack into the release
workspace. The example depends on the published `open-gpui-*` 0.2 crates while
using local Jellyflow path dependencies for the unreleased Jellyflow workspace
crates.

Run it from the Jellyflow root:

```sh
cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow
```

Run the focused tests:

```sh
RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow --no-fail-fast --status-level fail --final-status-level fail -E 'not test(gallery_screenshot::product_gallery_screenshot_exporter_writes_nonblank_pngs_or_skips)'
```

The hard adapter contracts remain in `jellyflow-open-gpui`. This example owns
the concrete Open GPUI component tree, node component kit, focus/popup lifecycle,
measured element collection, and product renderer polish.
