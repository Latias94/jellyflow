# Jellyflow Headless Adapter Template

This crate is a copyable starting point for a renderer or UI adapter. It stays outside the
Jellyflow workspace so it behaves like an external consumer.

The template runs Jellyflow's headless conformance layer before any renderer-specific smoke tests:

```text
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check <fixture-dir>
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- approve <fixture-dir>
```

Use the built-in suite as the first CI gate for adapter input translation. It covers node drag,
viewport pan, viewport animation frame sampling, and double-click zoom planning before rendering.
Keep renderer frame loops, animation cancellation policy, `wgpu`, egui, Fret, screenshots, and
pixel checks in adapter-specific crates or test suites.

When copying this template into another repository, replace the path dependencies in `Cargo.toml`
with the Jellyflow version you want to consume.
