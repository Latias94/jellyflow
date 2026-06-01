# Jellyflow Viewport Interaction Kernel v1 - Handoff

Status: Active
Last updated: 2026-06-01

## Current State

The workstream is open as a follow-on to the closed conformance fixture lane and the node drag
kernel lane.

JVI-010 is complete: the lane scope, non-goals, source coverage, task ledger, machine-readable task
state, draft campaign, milestones, gate set, context manifest, and workstream metadata are recorded.

## Next Task

JVI-020: add renderer-neutral viewport pan/zoom request types and deterministic transform helpers.

## Decisions Since Opening

- Runtime accepts normalized viewport intent; adapters own platform event capture.
- Keep v1 deterministic and immediate; animation and smoothing are follow-ons.
- Use the conformance fixture runner for viewport traces after the kernel and store callbacks exist.
- Renderer smoke tests stay outside `jellyflow-runtime`.

## Blockers

- None known.

## Follow-On Candidates

- Viewport animation/smoothing policy.
- Auto-pan integration with drag/select gestures.
- Adapter crate runner helpers for future wgpu, egui, Fret, or other integrations.
- Renderer smoke tests outside `jellyflow-runtime`.
