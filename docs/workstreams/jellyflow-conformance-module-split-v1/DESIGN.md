# Jellyflow Conformance Module Split v1 - Design

Status: Closed
Date: 2026-06-01

## Problem

`runtime::conformance` has grown into a 1300+ line module after adding scenario vocabulary, runner
execution, trace recording, file-backed fixtures, directory discovery, explicit approval, and the
example harness workflow. The public API is useful, but the implementation boundary is now too
wide for agents to navigate safely before adding more gesture families or adapter-owned smoke
tests.

## Target State

- `runtime::conformance::mod` becomes a small facade that preserves the existing public API.
- Scenario/schema vocabulary lives separately from runner execution.
- Reports and display formatting live separately from fixture IO and approval write-back.
- File/directory fixture helpers and approval helpers keep their existing behavior.
- Public imports used by runtime tests, examples, and external users remain source-compatible.

Target state is complete as of 2026-06-01.

## Scope

- Split `crates/jellyflow-runtime/src/runtime/conformance/mod.rs` into focused submodules.
- Preserve public type/function names and module path `jellyflow_runtime::runtime::conformance::*`.
- Move private helpers to the modules that own their behavior.
- Update workstream evidence and closeout docs.

## Non-Goals

- No fixture schema changes.
- No JSON format changes.
- No new runtime behavior.
- No renderer, GPU, screenshot, pixel, wgpu, egui, or Fret adapter code.
- No new dependencies.

## Architecture Direction

Use a facade plus owned submodules:

1. `mod.rs`: documentation, module declarations, and public re-exports.
2. `scenario.rs`: scenarios, suites, setup, trace config, actions, trace event vocabulary.
3. `runner.rs`: runner construction, action execution, trace recorder, callback recorder.
4. `reports.rs`: run reports, suite reports, mismatch types, run errors, display formatting.
5. `fixtures.rs`: suite file, directory discovery, fixture reports, fixture IO errors.
6. `approval.rs`: approval value types, approval reports, approval write-back behavior.

The split should be mechanical and behavior-preserving. If public-surface tests fail, prefer adding
explicit re-exports over changing call sites.
