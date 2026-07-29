# Native Node Authoring Regression Gates

This document lists the executable contracts for Jellyflow's framework-native node authoring path.
The supported architecture is:

```text
GraphRuntime -> ViewSession -> native adapter
```

Correctness comes from graph facts, version-checked commands, complete measured geometry, and native
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
text input and candidate rows, projects semantic candidates through its validated product catalog,
and verifies loading without blocking controls, empty results, source error/retry, stale
generations, Escape, outside-click cancellation, and host-close cancellation. A future adapter can
build a different search surface without taking on this UI or introducing a shared widget DSL.

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
bootstrap draw as success. The measured surface must be positive and finite. Retained lifecycle
evidence is visibility-aware: every visible expected node must be active, parked nodes may account
only for currently invisible nodes, active plus parked and strong-instance counts must equal the
fixture node count, and the initial mount count must never grow. Evictions, hard retirements,
protected-capacity overflow, and frame-materialization overflow are forbidden. At least two
feedback submissions must have completed; feedback, layout, and measurement failure counters must
remain zero; and the host must have no diagnostic or close reason.

The gate runs the production `open_gpui_platform::application()` entrypoint, not a test platform.
Each process emits one `JELLYFLOW_REAL_WINDOW_SMOKE` marker containing the named bounds, scenario,
configured and observed zoom evidence, initial fixture topology, current checkpoint geometry
topology, surface dimensions, retained lifecycle counts, feedback counts, and diagnostic state.
Log consumers must match a line beginning exactly with
`JELLYFLOW_REAL_WINDOW_SMOKE status=` and must also require a zero process exit code. The
`JELLYFLOW_REAL_WINDOW_SMOKE_AUTOPAN` failure diagnostic is not a result marker.
The marker serializes typed interaction or LOD evidence from the configured scenario. Evidence that
was never observed remains `missing`; it is never replaced with compact, interaction, or zoom-1
defaults. Capture totals are derived from unique named checkpoints, so a duplicate checkpoint fails
the process instead of inflating its count.
Shader and Workflow run the full interaction path at compact
`960x640` bounds with zooms `0.5`, `1`, and `2`, plus one normal `1440x900` run at zoom `1`. The
configured zoom remains active for every dispatched interaction and all nine checkpoints; the
native wheel probe restores that configured zoom rather than restoring a fit transform. The
interaction path also proves a Tab/Shift-Tab focus round trip and at least three viewport-changing
auto-pan frames while a node drag remains held in the 12-pixel edge safe area. Connection creation
uses the same fixed-zoom interaction model: it drags from the source to the target-facing 12-pixel
edge, holds for at least three viewport-changing frames, then moves to the now-visible target and
releases. This keeps distant 2x endpoints reachable without restoring fit.
The selected-node checkpoint repositions the dragged node without changing zoom so its published
bottom edge and the empty area below it are both visible. Pixel evidence must find one focus or
selection outline fitted to the current published node bounds, including the bottom edge, and must
not find a sustained second outline below those bounds. This turns the detached empty selection
box regression into a process failure rather than leaving it to artifact review.

A separate normal Shader process samples semantic LOD at `0.86`, `0.85`, and `0.84`. It compares a
stable node's screen-space bounds and every real port's node-relative route anchor across adjacent
samples, captures all three frames, and fails on a presentation jump. Keeping this directed probe
outside the product/bounds/zoom interaction matrix avoids a Cartesian expansion. Interaction
checkpoints validate complete graph geometry plus nonblank paint for every visible node; the process
marker and exit status remain the behavioral authority, while CI uploads PNGs for visual inspection
of overlap, endpoint shifts, and route discontinuity. The workflow starts from an empty capture
directory, requires the exact 75-file matrix rather than only its count, and uploads its sorted
manifest with the images.

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
cargo test --manifest-path examples/canvas-jellyflow/Cargo.toml --workspace --doc --locked
cargo clippy --manifest-path examples/canvas-jellyflow/Cargo.toml \
  --workspace --all-targets --locked -- -D warnings

# Prove production Window::on_next_frame drives node-drag auto-pan without test support.
cargo build --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked --no-default-features \
  --bin open-gpui-native-frame-clock-witness
python3 tools/check_open_gpui_native_frame_clock.py --require-native

# Build once, then run the bounded visual and interaction matrix through the CI orchestrator.
cargo build --manifest-path examples/canvas-jellyflow/Cargo.toml \
  -p open-gpui-canvas-jellyflow --locked --features real-window-smoke
set -euo pipefail
capture_output="$(mktemp)"
trap 'rm -f "$capture_output"' EXIT
python3 tools/run_real_window_smoke_matrix.py \
  --binary examples/canvas-jellyflow/target/debug/open-gpui-canvas-jellyflow \
  --capture-root target/real-window-smoke \
  --timeout-seconds 60 \
  --capture-dir-output "$capture_output"
python3 tools/check_real_window_smoke_evidence.py \
  --capture-dir-from-output "$capture_output"
```

The Open GPUI executable is the only product example. `jellyflow-egui` and the external
adapter-conformance consumer prove the independent framework boundary. The authoring obligations for
future adapters are documented in `docs/guides/native-adapter-authoring.md` and backed by executable
tests rather than a framework-free implementation.
