# Jellyflow Conformance Fixtures v1 - Handoff

Status: Active
Last updated: 2026-06-01

## Current State

The workstream is open as a follow-on to the closed interaction harness and node drag kernel lanes.

JCF-010 is complete: the lane scope, non-goals, source coverage, task ledger, milestones, gate set,
context manifest, and machine-readable workstream metadata are recorded.

## Next Task

JCF-020: define the first public headless conformance fixture vocabulary for graph setup,
view/config setup, actions, gestures, and expected normalized trace events.

## Decisions Since Opening

- Keep fixture execution renderer-free; adapters own pointer capture, windows, DOM/class filtering,
  screenshots, and pixels.
- Start with existing connect and node drag scenarios. Do not design an abstract scripting language.
- Public fixture API should be small and behavior-oriented, with public surface smoke coverage.
- Future adapter smoke tests can consume the fixture vocabulary but should live outside
  `jellyflow-runtime`.

## Blockers

- None known.

## Follow-On Candidates

- File-backed golden fixture corpus after in-code fixture types settle.
- Adapter crate runner helpers for future wgpu, egui, Fret, or other integrations.
- Broader gesture families such as resize, reconnect gesture lifecycle, and pan/zoom.
