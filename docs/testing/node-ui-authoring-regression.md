# Node UI Authoring Regression Contract

Jellyflow uses the cheapest deterministic layer that can own each invariant. A test must not
reimplement the product behavior it is trying to verify.

## Ownership by layer

### Core and runtime tests

Headless tests own graph validity, versioned commands, mutation settlement, undo/redo, geometry
publication, routing queries, interaction state, semantic zoom policy, and view retirement. These
tests use graph and view facts directly and do not depend on a native UI framework.

### Open GPUI adapter and product tests

Open GPUI tests own retained mount lifecycle, native event reduction, focus and pointer capture,
port measurement, content-size submission, node drag, connection/reconnect, search, selection
chrome bounds, edge painting plans, and product control authoring.

These tests may use Open GPUI's deterministic test context and measured element bounds. They must
not create a second renderer model, an input-planning DSL, or a framebuffer color classifier.
Precise LOD thresholds, zoom behavior, and layout coordinates belong here rather than in a real
window process matrix.

### Real-window smoke

The real-window smoke owns one narrow platform boundary: the production application can open a
native Shader or Workflow window and reach a valid first scene.

A process passes when all of the following are true before its internal 15-second timeout:

- the native view remains open;
- the host reports a finite, positive surface size and no diagnostic;
- the fixture graph contains nodes;
- at least one retained node is active and has published geometry;
- graphs with ports or edges publish at least one port or route respectively.

The process writes diagnostics to stderr and exits zero or nonzero. There is no JSON receipt,
results schema, screenshot matrix, synthetic interaction planner, pixel oracle, or external process
supervisor. Window-manager adjustments are not graph correctness failures.

Run both products after building the binary:

```text
cargo build --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --bin open-gpui-real-window-smoke \
  --features real-window-smoke --locked
examples/canvas-jellyflow/target/debug/open-gpui-real-window-smoke
examples/canvas-jellyflow/target/debug/open-gpui-real-window-smoke --workflow
```

## Visual quality

Screenshots are useful for human review of spacing, wire shape, port markers, typography, and
product style, but compositor pixels are not a stable cross-platform semantic contract. A visual
bug should first receive a focused measured-bounds or paint-plan regression test, followed by a
manual run of the affected product example.

## Required gates

```text
cargo fmt --all --check
cargo nextest run --workspace --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings

cargo fmt --manifest-path examples/canvas-jellyflow/Cargo.toml --all -- --check
cargo nextest run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  --workspace --all-features --locked
cargo test --manifest-path examples/canvas-jellyflow/Cargo.toml \
  --workspace --all-features --doc --locked
cargo clippy --manifest-path examples/canvas-jellyflow/Cargo.toml \
  --workspace --all-targets --all-features --locked -- -D warnings
```

The real-window checks run only on macOS CI because they validate the native platform integration,
not portable graph semantics. Full interaction coverage remains in deterministic Rust tests.
