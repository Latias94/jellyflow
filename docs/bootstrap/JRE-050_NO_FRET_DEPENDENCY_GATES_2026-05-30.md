# JRE-050 No-Fret Dependency Gates

Date: 2026-05-30
Status: Complete

## Scope

This task formalizes standalone dependency gates for Jellyflow packages.

## Changes

- Added `tools/check_no_fret_dependencies.py` for per-package `cargo tree` checks.
- Kept `tools/check_external_consumer_smoke.py` as the outside-workspace consumer smoke.

## Validation

- `cargo tree -p jellyflow-core --depth 2`: passed; no Fret packages present.
- `cargo tree -p jellyflow-runtime --depth 2`: passed; no Fret packages present.
- `python3 tools/check_no_fret_dependencies.py`: passed for `jellyflow-core` and
  `jellyflow-runtime`.
- `python3 tools/check_external_consumer_smoke.py`: passed; external cargo tree contained no Fret
  packages.
