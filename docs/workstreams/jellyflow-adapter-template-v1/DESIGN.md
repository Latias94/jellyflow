# Jellyflow Adapter Template v1 - Design

Status: Closed
Last updated: 2026-06-02

## Why This Lane Exists

Jellyflow now has headless conformance scenarios, suite reports, JSON fixture loading, directory
discovery, approval helpers, and an example CLI harness. The next missing adapter-facing artifact
is a copyable external crate template that shows how an adapter repo should consume those APIs
before it adds renderer-specific tests.

## Relevant Authority

- ADRs:
  - `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- Existing docs:
  - `README.md`
  - `crates/jellyflow-runtime/README.md`
- Related workstreams:
  - `docs/workstreams/jellyflow-adapter-conformance-runner-v1`
  - `docs/workstreams/jellyflow-conformance-file-fixtures-v1`
  - `docs/workstreams/jellyflow-conformance-fixture-discovery-v1`
  - `docs/workstreams/jellyflow-conformance-golden-approval-v1`

## Problem

External adapter authors can use the public conformance APIs, but they still have to infer how to
structure a real adapter crate: where to keep headless fixtures, how to run them in CI, how to prove
the crate stays free of Fret and renderer dependencies, and where renderer smoke tests should start.

Agents also need a stable, repository-owned template that can be exercised with `cargo` as if it
were an external consumer. Temporary smoke projects prove API usability, but they are not enough as
a copyable integration pattern.

## Target State

- The repository contains a non-workspace `templates/headless-adapter` crate that depends on
  Jellyflow through public APIs only.
- The template exposes a small adapter conformance suite and a CLI entry point for checking either
  built-in smoke scenarios or a fixture directory.
- Template tests prove the headless suite passes before any renderer is involved.
- Root/runtime docs point adapter authors at the template and keep `wgpu`, egui, Fret, screenshots,
  and pixel checks outside `jellyflow-core` and `jellyflow-runtime`.
- Tooling or smoke checks exercise the template as an external consumer and fail if it pulls Fret
  packages.

## In Scope

- A copyable adapter template crate outside the workspace member list.
- Template README material and cargo commands.
- Headless conformance suite construction using `ConformanceScenario`, `ConformanceSuite`, and
  `ConformanceFixtureDirectory`.
- External smoke coverage that runs the template with `cargo --manifest-path`.
- No-Fret dependency checks for the template dependency tree.

## Out Of Scope

- A production `jellyflow-wgpu`, `jellyflow-egui`, or Fret adapter crate.
- Renderer frame loops, `winit`, screenshots, pixel tests, GPU resources, or browser automation.
- Fixture schema changes.
- New gesture kernels such as parent expansion, double-click zoom, or pan inertia.
- Publishing metadata for the template as a package.

## Starting Assumptions

| Assumption | Confidence | Evidence | Consequence if wrong |
| --- | --- | --- | --- |
| A non-workspace template best represents an external adapter consumer. | High | `tools/check_external_consumer_smoke.py` already validates temp crates outside the workspace. | If workspace membership is required, the template may accidentally inherit workspace-only assumptions. |
| The first template should stay headless and renderer-free. | High | ADR 0003 keeps renderer smoke tests outside `jellyflow-core` and `jellyflow-runtime`. | If renderer behavior is required, this lane should split a renderer adapter workstream. |
| Programmatic smoke scenarios are enough for the first template slice. | Medium | File-backed fixture loaders and approval helpers already exist; the template can add durable fixtures after the crate skeleton is stable. | If users need committed golden JSON immediately, add a follow-up task for fixture assets. |

## Architecture Direction

Place the template under `templates/headless-adapter` rather than `crates/`. It is intentionally not
a workspace member, so tests must use `cargo --manifest-path` and the dependency tree looks like an
external adapter crate. The template should use path dependencies in-repo with README guidance that
real consumers can switch them to published versions when Jellyflow is released.

The template owns adapter-facing code and tests. It consumes `jellyflow-core` and
`jellyflow-runtime`; it does not add dependencies back into either crate. Renderer smoke tests
remain future adapter-specific work.

## Closeout Condition

This lane can close when:

- the template crate runs its headless suite and fixture-directory check path,
- documentation shows adapter authors how to use the template,
- external smoke and no-Fret gates cover the template,
- `cargo fmt`, relevant `cargo nextest` gates, and `git diff --check` pass,
- and remaining renderer-specific work is split or explicitly deferred.
