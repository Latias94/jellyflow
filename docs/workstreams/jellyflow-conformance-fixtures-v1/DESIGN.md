# Jellyflow Conformance Fixtures v1 - Design

Status: Active
Date: 2026-06-01

## Problem

Jellyflow now has renderer-neutral selection, connect, and node drag behavior covered by private
adapter-conformance tests. Those tests are useful for the runtime crate, but they are not yet a
stable fixture format that adapters, examples, or agents can reuse to catch regressions.

Without a shared fixture contract, each future Fret, egui, wgpu, or custom Rust adapter has to
invent its own way to prove "this feels like XyFlow" and agents have fewer compact, repeatable
signals for finding interaction regressions.

## Target State

Create a mature headless conformance fixture layer that can describe and replay interaction
scenarios without renderer dependencies:

- a stable fixture vocabulary for graph setup, view/config setup, gesture intent, runtime actions,
  and expected normalized traces;
- a runner that executes fixtures against a real `NodeGraphStore`;
- coverage for existing connect and node drag behavior before adding more gesture families;
- compact failure output suitable for humans and agents;
- documentation that tells adapter authors how to use the fixtures before writing renderer smoke
  tests.

## Scope

- Define a fixture model for runtime conformance scenarios.
- Reuse existing private harness semantics and normalized trace events.
- Convert existing connect and node drag adapter-conformance scenarios to the fixture runner.
- Keep fixture execution headless and deterministic.
- Document how adapters can reuse the fixture model and where renderer smoke tests belong.

## Non-Goals

- Do not add `wgpu`, `winit`, egui, Fret UI, screenshot, pixel-test, browser, or DOM dependencies.
- Do not make adapter crates part of this runtime lane.
- Do not encode renderer input capture, drag handles, CSS selectors, or platform quirks in the
  fixture schema.
- Do not publish a separate fixture crate in v1 unless the runtime surface proves too heavy.
- Do not replace focused Rust tests; fixtures should supplement behavior tests, not hide logic.

## Architecture Direction

The fixture layer should sit above the runtime store and below renderer adapters:

```text
fixture scenario -> conformance runner -> NodeGraphStore/runtime kernels -> normalized trace
```

The same fixture vocabulary should be able to drive private runtime tests and future adapter smoke
tests. Renderer adapters may translate their own input events into fixture-like normalized intent,
but fixture execution itself remains pure Rust.

## Source Coverage

| Source | State | Evidence | Impact |
| --- | --- | --- | --- |
| User goal | COVERED | Request for mature automated harnesses that help agents find issues | Fixture format becomes the next testing primitive. |
| ADR 0003 | COVERED | `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md` | Keeps conformance renderer-free and fixture-oriented. |
| Interaction harness lane | COVERED | `docs/workstreams/jellyflow-interaction-harness-v1/CLOSEOUT_AUDIT_2026-06-01.md` | Supplies private harness and normalized trace language. |
| Node drag lane | COVERED | `docs/workstreams/jellyflow-node-drag-kernel-v1/CLOSEOUT_AUDIT_2026-06-01.md` | Supplies drag scenarios and follow-on decision for public fixtures. |
| Runtime public surface lane | COVERED | `docs/workstreams/jellyflow-runtime-public-surface-v1/CLOSEOUT_AUDIT_2026-05-30.md` | Guides public API exposure discipline. |

## Risk Notes

- A fixture schema that is too general will become a second scripting language. Start with the
  existing connect and drag scenarios.
- Public fixture naming can freeze early. Keep v1 names explicit and behavior-oriented.
- Golden trace files can become noisy. Prefer normalized structured assertions with compact diffs.
