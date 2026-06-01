# Jellyflow Runtime Test Surface Split v1 - Design

Status: Closed
Date: 2026-06-01

## Why This Lane Exists

Jellyflow's runtime behavior is now protected by a broad test suite for conformance, drag,
adapter-facing traces, store dispatch, callbacks, geometry, and XyFlow-style projections. That is
good for safety, but several runtime test files have become large enough that future agents need to
scan too much unrelated behavior before making a narrow change.

This lane keeps the production runtime untouched and refactors the test surface so behavior
contracts are easier to find, extend, and review before richer gesture kernels or adapter
conformance fixtures arrive.

## Relevant Authority

- ADRs:
  - `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
  - `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- Existing docs:
  - `README.md`
  - `crates/jellyflow-runtime/README.md`
- Related workstreams:
  - `docs/workstreams/jellyflow-interaction-harness-v1/`
  - `docs/workstreams/jellyflow-conformance-schema-runner-split-v1/`
  - `docs/workstreams/jellyflow-node-drag-module-split-v1/`
  - `docs/workstreams/jellyflow-viewport-interaction-kernel-v1/`

## Problem

The largest remaining runtime maintenance friction is test organization rather than production code
shape. Files such as `runtime/tests/conformance.rs`, `runtime/tests/drag.rs`,
`runtime/tests/adapter_conformance.rs`, and `runtime/tests/harness.rs` mix fixtures, builders,
assertions, scenario cases, and behavior-specific expectations. This slows review and makes it easy
to add new gesture coverage in the wrong place.

## Target State

- Runtime conformance and adapter-facing tests are grouped by behavior and scenario family.
- Shared test helpers are small, named, and local to runtime tests.
- Test function names, common nextest filters, fixture JSON shape, callback traces, and public
  runtime APIs remain stable.
- Future gesture-kernel and adapter-conformance work has obvious test locations.
- Validation proves this is a test-only organization change.

## In Scope

- Refactor runtime test files under `crates/jellyflow-runtime/src/runtime/tests/`.
- Split conformance, adapter conformance, drag, and harness tests into focused submodules where it
  improves locality.
- Extract duplicate runtime-test setup, fixture builders, and assertions only when the local tests
  already repeat the same shape.
- Preserve existing production behavior, public API paths, fixture schema, and snapshots/traces.
- Update this workstream's evidence and closeout notes.

## Out Of Scope

- No production runtime, core model, schema, or rule behavior changes.
- No new conformance fixture action vocabulary.
- No fixture schema version bump.
- No `wgpu`, egui, Fret, screenshot, pixel, browser, or renderer smoke-test dependency.
- No broad core-test split; `jellyflow-core` test organization can be a separate follow-on.
- No delete planner, resize kernel, reconnect lifecycle kernel, or pan/zoom behavior work.

## Starting Assumptions

| Assumption | Confidence | Evidence | Consequence if wrong |
| --- | --- | --- | --- |
| Runtime production modules are no longer the highest-value refactor target. | High | Recent module-split workstreams closed with behavior-preserving evidence. | Re-run the fearless-refactor audit and open a narrower production-module lane. |
| The remaining runtime friction is mostly test navigation and helper locality. | High | Largest runtime files are test files and harness files. | Narrow the lane to only the proven oversized files. |
| Test reorganization can preserve existing filters and behavior. | Medium | Prior module splits preserved public surface through facades and focused gates. | Prefer smaller submodules and explicit `mod` facades over renaming tests. |
| Renderer smoke tests belong outside runtime. | High | ADR 0003 keeps `jellyflow-core` and `jellyflow-runtime` renderer-free. | Split an adapter-crate lane instead of expanding this one. |

## Architecture Direction

Use test-only facades and focused helper modules:

1. Keep `crates/jellyflow-runtime/src/runtime/tests.rs` and existing parent test-module paths as
   stable entry points.
2. Split large test files into directory modules only when the split maps to behavior families:
   conformance scenarios, adapter trace expectations, drag planning cases, and harness assertions.
3. Keep helper extraction local to runtime tests. Do not move helpers into production modules just
   because several tests use them.
4. Preserve current public-surface and conformance gates so a test-only refactor cannot silently
   change runtime behavior.

## Closeout Condition

This lane can close when:

- runtime test files are easier to navigate and have clear ownership boundaries,
- the selected large test files are split or explicitly deferred,
- all required runtime package gates pass,
- evidence records that no production API, fixture schema, or renderer boundary changed,
- and follow-on work is either split or explicitly deferred.

## Outcome

Closed on 2026-06-01. The runtime test surface is now organized around focused test-only modules:

- conformance runner, file I/O, approval, and support modules;
- adapter conformance fixture-runner, projection, geometry, and support modules;
- harness event, interaction, and callback-recorder modules;
- drag single-node, multi-selection, and support modules;
- selection-box and support modules.

`viewport.rs` and `auto_pan.rs` remain direct modules because their current size and ownership are
still clear. The lane did not change production runtime behavior, public API paths, fixture JSON
schema, conformance trace semantics, callback payloads, or renderer boundaries.
