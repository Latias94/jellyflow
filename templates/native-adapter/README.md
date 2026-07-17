# Jellyflow Native Adapter Template

This standalone crate is the smallest supported boundary for a third-party native UI adapter. It
uses the public `jellyflow` facade and contains no UI framework, retained-host implementation,
portable widget vocabulary, or compatibility layer. Replace the test measurements with bounds from
the target framework's real layout or response APIs.

The adapter loop is:

1. Build an application-owned `NodeCatalog` containing semantic kinds and real port declarations.
2. Create one `GraphRuntime` and attach one `ViewSession` per native canvas.
3. Request an owned `AdapterFrame` using the view's current adapter environment.
4. Render ordinary framework-native components for the admitted frame nodes.
5. After native layout releases framework borrows, call `FrameNode::begin_layout` and record one
   `Rendered`, `Hidden`, `Culled`, or `Unavailable` outcome for every real port.
6. Append native event results and complete layouts to the frame's `FeedbackCollector` in the order
   they occurred.
7. Finish the collector once and submit it through the originating `ViewSession`.

Commands and layouts intentionally share one log. If a preceding topology command changes a node,
layout sampled from that old frame becomes stale and the adapter requests a fresh frame. Never
resubmit the consumed feedback value or synchronously mutate the runtime from a framework render
borrow; retain application-level intent and rebuild it against a fresh owned snapshot.

Dynamic handles must be real graph `Port` records with stable `PortId`s. A visually suppressed port
is `Hidden`, a viewport-pruned port is `Culled`, and a real port without a usable measurement is
`Unavailable`. Only `Rendered` ports participate in hit testing and routing. Omitting a real port is
an adapter protocol error.

This template does not copy Jellyflow's retained Open GPUI host, cache policy, or activity leases.
Those are framework-specific responsibilities. Open GPUI applications should use the published
`jellyflow-open-gpui` registry, scene host, binding command tickets, and port measurement wrappers;
another framework should implement its own native lifecycle directly over the same owned protocol.

Run the focused gate from the repository root:

```sh
cargo fmt --manifest-path templates/native-adapter/Cargo.toml --all -- --check
cargo nextest run --manifest-path templates/native-adapter/Cargo.toml --locked
cargo clippy --manifest-path templates/native-adapter/Cargo.toml \
  --workspace --all-targets --locked -- -D warnings
```
