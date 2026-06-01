# Jellyflow Interaction Harness v1 - Design

Status: Active
Date: 2026-06-01

## Problem

Jellyflow has strong headless graph, store, policy, geometry, and adapter-conformance tests, but
the "XyFlow feel" checks are still mostly handwritten per scenario. That makes future interaction
work risky for both humans and agents:

- scenario setup is repeated and easy to skew;
- event ordering assertions are local and inconsistent;
- failures do not yet produce a compact interaction trace;
- renderer adapters do not have one obvious conformance harness to reuse;
- new gesture kernels can accidentally test implementation details instead of observable behavior.

ADR 0003 already states that adapter conformance should be renderer-independent and fixture-based.
This lane turns that decision into a concrete test harness.

## Target State

Add a reusable runtime test harness that can drive a real `NodeGraphStore` and record a normalized
trace of observable behavior:

- graph commits with transaction labels and operation kinds;
- view changes with ordered viewport and selection updates;
- gesture lifecycle events emitted through the runtime store;
- optional XyFlow compatibility projections where a scenario needs them;
- clear scenario names and assertion messages suitable for automated agent debugging.

The first executable slice is intentionally test-only. It should deepen the existing conformance
tests without committing Jellyflow to a public fixture format too early.

## Scope

- Create `crates/jellyflow-runtime/src/runtime/tests/harness.rs` as the shared test harness.
- Register the harness in `runtime/tests.rs`.
- Migrate or add adapter-conformance scenarios to prove the harness records graph, view, and
  gesture behavior in order.
- Keep all tests renderer-free and dependency-free.
- Record gates and evidence in this workstream.

## Non-Goals

- Do not add `wgpu`, `winit`, egui, Fret UI, browser, screenshot, or pixel-test dependencies.
- Do not expose a public test-fixture crate in this lane.
- Do not implement the full drag, pan/zoom, resize, or reconnect state machines yet.
- Do not move persisted graph policy/layout fields out of `Graph`.
- Do not add snapshot-test dependencies until plain Rust assertions prove insufficient.

## Architecture Direction

The harness should sit at the runtime behavior boundary:

```text
fixture/intent -> NodeGraphStore public/runtime APIs -> normalized trace -> expected trace
```

It should avoid testing private internals. A future public harness can be extracted only after the
private test harness survives several interaction-kernel slices.

## Source Coverage

| Source | State | Evidence | Impact |
| --- | --- | --- | --- |
| User goal | COVERED | Conversation on automated harness for self-correction and agent debugging | Harness is the first-class deliverable. |
| ADR 0003 | COVERED | `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md` | Keeps tests renderer-free and fixture-oriented. |
| Runtime public surface lane | COVERED | `docs/workstreams/jellyflow-runtime-public-surface-v1/` | Confirms canonical runtime APIs and compatibility boundaries. |
| Geometry/spatial lane | COVERED | `docs/workstreams/jellyflow-geometry-spatial-v1/` | Provides hit-test and path primitives future gesture fixtures can use. |
| Existing conformance tests | COVERED | `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance.rs` | First migration target. |
| XyFlow reference code | DEFERRED | `repo-ref/xyflow/packages/system/src/` | Useful for later gesture kernels, not required for the harness tracer bullet. |

## Risk Notes

- A public fixture format too early would freeze weak names. Keep the first harness private to
  runtime tests.
- Snapshot tests can hide weak assertions. Prefer explicit normalized traces for now.
- The harness should help agents find mismatches quickly, so assertion failures need scenario names
  and compact expected/actual trace output.
