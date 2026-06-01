# Jellyflow Conformance Golden Approval v1 - Design

Status: Closed
Date: 2026-06-01

## Problem

Jellyflow can load conformance suite files and discover fixture directories, but adapters and agents
still lack a controlled headless way to refresh golden `expected_trace` values from actual runtime
behavior.

## Target State

- Runtime conformance can produce an approved suite by replacing each scenario's `expected_trace`
  with the actual trace observed from a real `NodeGraphStore` run.
- Approval reports state which scenarios changed and which scenarios errored.
- File and directory helpers can explicitly write approved traces back to JSON fixture files.
- Write-back refuses to proceed when scenario execution errors exist.
- Reports are serde-friendly so agents can log approval decisions.
- The workflow remains renderer-free and does not add a CLI.

## Scope

- Add suite-level approval/update primitives under `runtime::conformance`.
- Add file and directory write-back helpers that save approved JSON only when execution is clean.
- Add focused tests for suite approval, file write-back, and directory write-back refusal on
  execution errors.
- Add public-surface smoke coverage.
- Update README/runtime README and close the lane.

## Non-Goals

- No CLI command or environment flag.
- No automatic approval without explicit API calls.
- No renderer screenshots, pixel assets, GPU, windowing, or adapter platform code.
- No schema migration or fixture directory convention changes.

## Architecture Direction

Build on the existing runner and fixture file APIs:

1. use `run_conformance_scenario` to capture actual traces;
2. keep original fixture files unchanged unless an explicit write-back helper is called;
3. reject write-back if any scenario execution error exists;
4. save approved suites through `ConformanceSuite::save_json`;
5. keep directory approval two-phase: compute approvals first, then write files.
