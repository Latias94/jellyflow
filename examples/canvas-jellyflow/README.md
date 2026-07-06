# Open GPUI Canvas Jellyflow Example

This package is Jellyflow's concrete Open GPUI product gallery.

It intentionally lives outside the Jellyflow root workspace so the default
Jellyflow checks do not require a local Open GPUI checkout. Until the required
`open-gpui-*` 0.2 crates are published, this example depends on a pinned Open
GPUI git revision. Using a path dependency to `repo-ref/open-gpui` from this
manifest conflicts with Cargo workspace inheritance because the example also
depends on local Jellyflow workspace crates.

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
