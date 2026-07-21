# Native Node Authoring Regression Gates

This document lists the executable contracts for Jellyflow's framework-native node authoring path.
The supported architecture is:

```text
GraphRuntime -> ViewSession -> native adapter
```

Correctness comes from graph facts, revisioned commands, complete measured geometry, and native
framework tests. Screenshots may support visual review, but they are not a substitute for these
contracts.

## Product coverage

| Surface | Required behavior | Primary evidence |
| --- | --- | --- |
| Dify workflow | native text/select controls, focus and IME ownership, retained-field conflict recovery, stable dynamic branch ports, exact undo/redo, and stale two-view replanning | `examples/canvas-jellyflow/tests/native_workflow_host.rs` |
| Shader / Blueprint | typed real ports, inline values derived from incident edges, dynamic pins, occupied-input replacement, reconnect, create-from-wire search, and structured rejection | `examples/canvas-jellyflow/tests/native_shader_host.rs` plus `crates/jellyflow-open-gpui/src/host_tests.rs` |
| Graph-symbol blackboard | application-owned native controls, stable `SymbolId`, explicit stale-draft reapply, and exact undo without a portable blackboard descriptor | `examples/canvas-jellyflow/tests/native_blackboard.rs` |
| Unknown renderer | diagnostic native fallback while opaque node data and all policy-controlled real ports remain intact | `crates/jellyflow-open-gpui/tests/measurement_feedback.rs` and registry tests |
| Real native window | non-headless Open GPUI launch, measured workflow and shader product frames, retained mounts, and successful native layout feedback | `open-gpui-canvas-jellyflow --smoke` in both demo modes |

The dropped-wire search is intentionally application-owned. The Open GPUI host exposes only the
revision-bound request, semantic candidate query, local overlay anchor, cancellation, and atomic
commit operations. The example supplies an asynchronous, cancellation-by-drop provider plus native
text input and candidate rows, filters against its own `NodeCatalog`, and verifies loading without
blocking controls, empty results, source error/retry, stale generations, Escape, outside-click
cancellation, and host-close cancellation. A future adapter can build a different search surface
without taking on this UI or introducing a shared widget DSL.

Dynamic handles are valid only when backed by stable graph `PortId`s. Product components must derive
connected state from `ViewSession::port_connection_facts` or binding reads; they must not maintain a
second `connected_inputs` mirror in node payload or component state.

## Adapter contracts

`jellyflow-open-gpui` owns retained registry, scene, binding, measurement, input arbitration,
connection lifecycle, draft reconciliation, activity leases, accessibility, fallback, and bounded
retention. Its tests cover:

- complete root and real-port measurement through native prepaint;
- one-shot, deferred, ordered feedback, stale withdrawal, and batch settlement across dropped or
  panicking frame preparation and outcome-unknown submission;
- control/port/node/canvas/overlay input precedence, command-prefix waiting, optimistic drag
  presentation, capture loss, successor ownership, late-release suppression, and cleanup;
- focused and IME-composing draft conflicts with accept, discard, and explicit reapply paths;
- culling, parking, hard retirement, protected-owner overflow, and terminal view closure;
- missing, duplicate, empty, and failing renderer registrations;
- stable accessibility roles, names, state, keyboard activation, and focus restoration;
- connection generation, search cancellation, freshness checks, replacement, reconnect, and command
  rejection without graph mutation;
- semantic zoom at `0.05`, `0.5`, `1`, and `2`: unprotected low-zoom nodes use adapter-owned
  overview proxies with complete real-port geometry, while focus, IME, capture, overlay, and draft
  leases preserve the retained native node at readable 1:1 pixels without remounting.

Open GPUI 0.2's test `PlatformWindow` does not retain the callbacks passed to `a11y_init` and leaves
`a11y_tree_update` as a no-op. The current automated gate therefore verifies the production semantic
element builders with `Element::write_a11y_info` and exercises keyboard activation, but it cannot
capture the platform `TreeUpdate` or inject an AccessKit `ActionRequest`. That platform integration
remains an explicit upstream test-support gap and is not represented as passing evidence here.

`jellyflow-egui` is deliberately narrower. It proves the same frame/layout/feedback protocol with
egui response geometry and immediate-mode state. It does not emulate retained entities or ship a
second product gallery.

## Real-window contract

The initial frame of `OpenGpuiSceneHost::new_measured_surface` bootstraps the native surface size.
The real-window gate therefore waits for the first complete product frame rather than treating that
bootstrap draw as success. The measured surface must be positive and finite; all expected fixture
nodes must be active with no parked or remounted instances; at least two feedback submissions must
have completed; feedback, layout, and measurement failure counters must remain zero; and the host
must have no diagnostic or close reason.

The gate runs the production `open_gpui_platform::application()` entrypoint, not a test platform.
Each process emits one `JELLYFLOW_REAL_WINDOW_SMOKE` marker containing the mode, surface dimensions,
retained lifecycle counts, feedback counts, and diagnostic state. Shader and Workflow run with
capture zooms `0.5`, `1`, and `2`. Each run records initial, selected, connection-preview,
connection-committed, reconnect-selected, reconnect-preview, reconnect-committed, search-open, and
final frames. Every checkpoint repeats complete geometry and nonblank per-node paint validation;
the process marker and exit status remain the behavioral authority, while CI uploads the PNG matrix
for visual inspection of overlap, extra selection boxes, endpoint shifts, and route discontinuity.

## Runtime contracts

The framework-free runtime tests are the authority for multi-view isolation, revision conflicts,
connection policy, dynamic topology, history, frame admission, complete geometry publication,
environment freshness, replay protection, incremental routing/index work, and 10,000-node scale.
Open GPUI exercises those contracts through its production scene host; egui exercises them through
`EguiNativeFrameHost` with real response geometry and native widget state.

## Required gates

Run the root workspace gates:

```sh
cargo fmt --all --check
cargo check --workspace --locked
cargo nextest run --workspace --locked --cargo-quiet
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 tools/check_no_fret_dependencies.py
python3 -m unittest discover -s tools/tests -p "test_*.py"
python3 tools/check_external_consumer_smoke.py
git diff --check
```

Run both real adapter witnesses directly:

```sh
cargo nextest run -p jellyflow-open-gpui --locked
cargo nextest run -p jellyflow-egui --locked
python3 tools/check_external_consumer_smoke.py
```

Run the retained Open GPUI product workspace separately:

```sh
cargo fmt --manifest-path examples/canvas-jellyflow/Cargo.toml --all -- --check
cargo check --manifest-path examples/canvas-jellyflow/Cargo.toml --workspace --locked
cargo nextest run --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked --no-fail-fast
cargo clippy --manifest-path examples/canvas-jellyflow/Cargo.toml \
  --workspace --all-targets --locked -- -D warnings

# Build once, then run the complete visual and interaction matrix.
cargo build --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked --features real-window-smoke
mkdir -p target/real-window-smoke
for zoom in 0.5 1 2; do
  JELLYFLOW_SMOKE_CAPTURE_DIR=target/real-window-smoke JELLYFLOW_SMOKE_ZOOM="$zoom" \
    python3 tools/run_with_timeout.py --timeout-seconds 60 -- \
    examples/canvas-jellyflow/target/debug/open-gpui-canvas-jellyflow --smoke
  JELLYFLOW_SMOKE_CAPTURE_DIR=target/real-window-smoke JELLYFLOW_SMOKE_ZOOM="$zoom" \
    python3 tools/run_with_timeout.py --timeout-seconds 60 -- \
    examples/canvas-jellyflow/target/debug/open-gpui-canvas-jellyflow --workflow --smoke
done
```

The Open GPUI executable is the only product example. `jellyflow-egui` and the external
adapter-conformance consumer prove the independent framework boundary. The authoring obligations for
future adapters are documented in `docs/guides/native-adapter-authoring.md` and backed by executable
tests rather than a framework-free implementation.
