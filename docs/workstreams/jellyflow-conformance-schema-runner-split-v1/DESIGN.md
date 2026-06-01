# Jellyflow Conformance Schema Runner Split v1 - Design

Status: Closed
Date: 2026-06-01

## Problem

`runtime::conformance` is now a module facade, but two internal files still mix several ownership
concerns:

- `scenario.rs` owns suite/scenario builders, setup state, action vocabulary, trace vocabulary, and
  callback event vocabulary.
- `runner.rs` owns suite iteration, store construction, action execution, store/gesture tracing,
  callback tracing, and graph-op serialization.

This is workable today, but richer gesture families such as resize, reconnect lifecycle,
selection-box, and pan/zoom fixtures will keep expanding the same files unless the schema and runner
boundaries are made deeper now.

## Target State

- `runtime::conformance::scenario` becomes a private facade over focused schema modules.
- Public `runtime::conformance::*` re-exports remain unchanged.
- Fixture JSON schema, serde tags, defaults, and schema version remain unchanged.
- Runner execution, trace collection, and callback recording live in focused private modules.
- Existing conformance tests, public-surface tests, example harness checks, and package gates pass.

## Scope

- Split `crates/jellyflow-runtime/src/runtime/conformance/scenario.rs` into a directory module with
  focused schema files.
- Split `crates/jellyflow-runtime/src/runtime/conformance/runner.rs` into a directory module with
  focused runner files.
- Preserve all public conformance API paths, fixture JSON shape, report behavior, callback traces,
  and error messages.
- Update workstream evidence and closeout docs.

## Non-Goals

- No new conformance fixture actions.
- No schema version bump.
- No renderer, adapter crate, screenshot, pixel, wgpu, egui, or Fret integration.
- No behavior changes to node drag, viewport, auto-pan, connect, reconnect, selection, or store
  dispatch semantics.

## Architecture Direction

Use private facades plus owned submodules:

1. `scenario/mod.rs`: schema facade and public re-exports for the private conformance module.
2. `scenario/constants.rs`: schema version and serde default helpers.
3. `scenario/suite.rs`: `ConformanceScenario`, `ConformanceSuite`, and builders.
4. `scenario/setup.rs`: `ConformanceSetup` and `ConformanceTraceConfig`.
5. `scenario/action.rs`: `ConformanceAction` and action constructors.
6. `scenario/trace.rs`: `ConformanceTraceEvent`, `ConformanceViewChange`, and
   `ConformanceCallbackEvent`.
7. `runner/mod.rs`: public runner facade and suite/scenario entry points.
8. `runner/actions.rs`: action execution against `NodeGraphStore`.
9. `runner/trace.rs`: store event and view-change trace projection.
10. `runner/callbacks.rs`: XyFlow callback trace recorder.

If public-surface or JSON fixture tests fail, prefer preserving re-exports and serde attributes over
renaming or reshaping call sites.

## Outcome

Closed on 2026-06-01. `runtime::conformance::scenario` and `runtime::conformance::runner` are now
private facades over focused submodules. Public `runtime::conformance::*` exports, fixture serde
schema, trace ordering, callback payloads, and runner error behavior remain unchanged.
