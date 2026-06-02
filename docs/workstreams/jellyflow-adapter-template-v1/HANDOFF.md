# Jellyflow Adapter Template v1 - Handoff

Status: Closed
Last updated: 2026-06-02

## Current State

JAT-010 through JAT-040 are complete. The workstream is closed.

## Current Task

None.

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

Open a new adapter-specific renderer smoke lane if wgpu, egui, Fret, screenshot, or pixel behavior
needs implementation. Keep those dependencies outside `jellyflow-core` and `jellyflow-runtime`.

## Follow-Ons

- Renderer-specific smoke lanes for future `jellyflow-wgpu`, `jellyflow-egui`, or Fret adapters.
- Committed golden JSON fixture assets in downstream adapter repos if programmatic template
  scenarios are not enough.
- Broader gesture-family templates after parent expansion, double-click zoom, or pan inertia
  kernels exist.
