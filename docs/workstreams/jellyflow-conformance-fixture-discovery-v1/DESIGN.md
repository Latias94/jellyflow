# Jellyflow Conformance Fixture Discovery v1 - Design

Status: Closed
Date: 2026-06-01

## Problem

`ConformanceSuite` can now load and save individual JSON fixture files, but adapter crates and
agents still need a deterministic way to discover many suite files from a fixture directory before
approval/update workflows exist.

## Target State

- Runtime conformance exposes a renderer-free directory loader for fixture suites.
- Discovery is deterministic and recursive under a root directory.
- Loaded suites retain their source file path for aggregate reports and future approval workflows.
- Missing fixture directories can be treated as optional.
- Directory, entry, file, parse, write, and serialize errors carry path context.
- Documentation explains that directory discovery is still headless and precedes renderer smoke
  tests.

## Scope

- Add a path-aware fixture directory type under `runtime::conformance`.
- Add focused tests for sorted recursive discovery, optional missing directories, and invalid file
  errors.
- Add public-surface smoke coverage.
- Update README/runtime README and close the lane.

## Non-Goals

- No golden approval or update workflow.
- No snapshot rewriting.
- No renderer screenshots, pixels, GPU, windowing, or platform adapter code.
- No implicit schema migrations for fixture JSON.

## Architecture Direction

Build on the file fixture helpers:

1. load each suite through `ConformanceSuite::load_json`;
2. discover `*.json` files recursively with `std::fs`;
3. sort paths before loading so agent and CI output is stable;
4. skip non-files and non-JSON paths;
5. keep reports path-aware but renderer-neutral.
