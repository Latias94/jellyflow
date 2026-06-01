# Jellyflow Conformance File Fixtures v1 - Design

Status: Closed
Date: 2026-06-01

## Problem

Jellyflow can now group headless conformance scenarios into suites and produce aggregate reports.
Adapters and agents still need a stable JSON file-backed path so fixture suites can live as durable
golden assets instead of being constructed only in Rust code.

## Target State

- `ConformanceSuite` can load and save pretty-printed JSON files.
- Missing fixture files can be treated as optional through an `if_exists` helper.
- Parse, read, write, and serialize errors carry file path context.
- Public-surface coverage proves external consumers can use the loader.
- Documentation states that golden fixture files are headless assets before renderer smoke tests.

## Scope

- Add suite JSON load/save helpers under `runtime::conformance`.
- Add focused tests for roundtrip load/save, optional missing files, and parse errors.
- Add public-surface smoke coverage.
- Update README/runtime README and close the lane.

## Non-Goals

- No fixture directory discovery convention.
- No approval-test workflow.
- No renderer screenshots or pixel fixture files.
- No platform-specific adapter runner code.

## Architecture Direction

Mirror existing runtime file helpers:

1. accept `impl AsRef<Path>`;
2. read/write JSON bytes with `std::fs`;
3. parse/serialize via `serde_json`;
4. preserve existing suite schema exactly;
5. keep execution separate from loading.
