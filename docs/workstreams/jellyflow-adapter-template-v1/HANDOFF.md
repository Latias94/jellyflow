# Jellyflow Adapter Template v1 - Handoff

Status: Active
Last updated: 2026-06-02

## Current State

JAT-010, JAT-020, and JAT-030 are complete. The workstream is open from the closed adapter
conformance runner lane's follow-on list.

## Current Task

JAT-040: record final evidence, close the lane, and split renderer-specific follow-ons.

## Context To Read

- `docs/adr/0003-headless-adapter-testing-and-renderer-boundary.md`
- `README.md`
- `crates/jellyflow-runtime/README.md`
- `docs/workstreams/jellyflow-adapter-template-v1/DESIGN.md`
- `docs/workstreams/jellyflow-adapter-template-v1/TODO.md`

## Stop Conditions

- The work requires `wgpu`, egui, Fret, `winit`, screenshot, pixel, or browser dependencies.
- The template requires fixture schema changes.
- Workspace-only assumptions are needed for the template to compile or run.
- Required gates fail.

## Completed This Session

- Added `templates/headless-adapter` as a non-workspace adapter template.
- Added built-in node drag and viewport pan conformance scenarios.
- Added template CLI commands for built-in checks and fixture-directory check/approve.
- Extended external smoke to run the template and check its dependency tree for Fret packages.
- Documented template commands in root and runtime READMEs.

## Next Likely Step

Run JAT-040 closeout gates, then close this workstream if no renderer-specific work is pulled in.
