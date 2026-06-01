# Jellyflow Adapter Conformance Runner v1 - Design

Status: Closed
Date: 2026-06-01

## Problem

Jellyflow now has reusable headless conformance scenarios for graph commits, view changes,
gestures, callbacks, viewport movement, node drag, and auto-pan. External adapters still need a
simple way to group those scenarios into a named suite and get one aggregate report.

Without a suite-level helper, each adapter crate will write local loops, local failure summaries,
and local continuation policy. That makes failures harder for agents to compare across Fret, egui,
wgpu, or other integrations.

## Target State

- `runtime::conformance` exposes a serializable `ConformanceSuite`.
- A suite can run all scenarios and return an aggregate `ConformanceSuiteReport`.
- Reports separate action execution errors from trace mismatches.
- The helper stays renderer-free and uses existing `ConformanceRunner` semantics.
- Public README material tells adapter authors to use suites before renderer smoke tests.

## Scope

- Add public suite/run/report types in `runtime::conformance`.
- Add focused tests for matched suites, mismatched suites, and scenario execution errors.
- Add public-surface smoke coverage.
- Update documentation and close the lane after fresh evidence.

## Non-Goals

- No file-system fixture loader in this lane.
- No golden snapshot format or approval workflow.
- No renderer frame-loop, screenshot, pixel, `wgpu`, egui, Fret, or platform dependency.
- No change to existing `ConformanceScenario` behavior.

## Architecture Direction

`ConformanceSuite` should be a thin orchestration object over existing scenarios:

1. run scenarios in input order;
2. collect successful `ConformanceRunReport` values;
3. collect `ConformanceRunError` values for scenarios that fail during action execution;
4. expose `is_match()` and compact display output for test failures.

This gives adapters a stable public test seam without introducing renderer assumptions.
