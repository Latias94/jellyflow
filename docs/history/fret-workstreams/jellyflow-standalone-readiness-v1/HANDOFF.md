# Jellyflow Standalone Readiness v1 - Handoff

Status: Active
Last updated: 2026-05-30

## Current State

This lane is open as a follow-on to `jellyflow-package-split-v1`. The package split lane proved the
in-workspace `jellyflow-core` and `jellyflow-runtime` boundaries, then closed without moving code to
a separate repository, publishing crates, removing compatibility re-exports, or extracting geometry.

JSR-010 is complete. The inventory found that the Jellyflow crates are free of UI/render/platform
dependencies but still depend on `fret-core` for small input/geometry types. That dependency must be
removed or consciously replaced before an external smoke can prove true standalone use.

## Assumptions

- Confident: standalone repository extraction should not start until package dependencies,
  metadata, documentation, compatibility policy, and external-consumer smoke are explicit.
- Confident: `fret-node` remains the Fret adapter and compatibility facade during this lane.
- Confident: geometry stays out of scope because the previous lane found it is still adapter-bound.
- Confident: the preferred future local repo path is `~/codes/rust/jellyflow`.
- Confident: JSR-030 should assume a new repository with history-preserving Git extraction as the
  default policy.
- Likely: the next useful step is detaching `fret-core`, not creating the new repo yet.

## Next Task

Start with JSR-015 in `TODO.md`: remove or consciously replace the `fret-core` dependency before
external smoke.

Use the JSR-010 inventory as the input. The concrete blockers are `fret_core::Modifiers`,
`fret_core::KeyCode`, and `fret_core::Rect`/`Point`/`Size`/`Px` usage in the Jellyflow crates. Keep
Fret-specific conversions in `fret-node`.
