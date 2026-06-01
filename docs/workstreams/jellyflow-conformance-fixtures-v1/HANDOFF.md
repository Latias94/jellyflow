# Jellyflow Conformance Fixtures v1 - Handoff

Status: Active
Last updated: 2026-06-01

## Current State

The workstream is active as a follow-on to the closed interaction harness and node drag kernel lanes.

JCF-010 is complete: the lane scope, non-goals, source coverage, task ledger, milestones, gate set,
context manifest, and machine-readable workstream metadata are recorded.

JCF-020 is complete: `jellyflow_runtime::runtime::conformance` now exposes renderer-free
scenario/setup/action/trace vocabulary, gesture and callback payloads are serde-friendly, and
`public_surface` protects a node-drag fixture round trip.

JCF-030 is complete: `run_conformance_scenario` executes fixtures against `NodeGraphStore`,
records normalized store/gesture/callback traces, and returns compact per-index mismatches.

JCF-040 is complete: connect dispatch, connect gesture lifecycle, connect gesture transaction
callbacks, and node drag gesture callbacks now run through `run_conformance_scenario`.

## Next Task

JCF-050: document the fixture strategy, record fresh closeout evidence, and close the lane or split
follow-ons.

## Decisions Since Opening

- Keep fixture execution renderer-free; adapters own pointer capture, windows, DOM/class filtering,
  screenshots, and pixels.
- Start with existing connect and node drag scenarios. Do not design an abstract scripting language.
- Public fixture API should be small and behavior-oriented, with public surface smoke coverage.
- Future adapter smoke tests can consume the fixture vocabulary but should live outside
  `jellyflow-runtime`.
- Fixture schema version starts at `CONFORMANCE_FIXTURE_SCHEMA_VERSION = 1`.
- JCF-020 intentionally defines the vocabulary only; execution, mismatch formatting, and
  conversion of existing scenarios remain in JCF-030/JCF-040.
- JCF-030 keeps runner actions renderer-free: dispatch transaction, apply node drag, set
  viewport/selection, and emit normalized gesture events.
- Reconnect, delete, viewport, and geometry adapter-conformance tests remain focused private
  harness/direct tests because JCF-040 only targeted connect and node drag fixture conversion.

## Blockers

- None known.

## Follow-On Candidates

- File-backed golden fixture corpus after in-code fixture types settle.
- Adapter crate runner helpers for future wgpu, egui, Fret, or other integrations.
- Broader gesture families such as resize, reconnect gesture lifecycle, and pan/zoom.
