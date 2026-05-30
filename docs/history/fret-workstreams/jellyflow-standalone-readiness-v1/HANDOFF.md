# Jellyflow Standalone Readiness v1 - Handoff

Status: Active
Last updated: 2026-05-30

## Current State

This lane is open as a follow-on to `jellyflow-package-split-v1`. The package split lane proved the
in-workspace `jellyflow-core` and `jellyflow-runtime` boundaries, then closed without moving code to
a separate repository, publishing crates, removing compatibility re-exports, or extracting geometry.

JSR-010 and JSR-015 are complete. The inventory found that the Jellyflow crates were already free of
UI/render/platform dependencies, and JSR-015 removed the remaining `fret-core` dependency by moving
small input/geometry contracts into Jellyflow-owned or direct external types.

## Assumptions

- Confident: standalone repository extraction should not start until package dependencies,
  metadata, documentation, compatibility policy, and external-consumer smoke are explicit.
- Confident: `fret-node` remains the Fret adapter and compatibility facade during this lane.
- Confident: geometry stays out of scope because the previous lane found it is still adapter-bound.
- Confident: the preferred future local repo path is `~/codes/rust/jellyflow`.
- Confident: JSR-030 should assume a new repository with history-preserving Git extraction as the
  default policy.
- Likely: the next useful step is an external consumer smoke, not creating the new repo yet.

## Next Task

Start with JSR-020 in `TODO.md`: prove an external headless consumer smoke.

Use the JSR-015 evidence as the input. The smoke should live outside the Fret workspace and should
path-depend only on `jellyflow-core` and `jellyflow-runtime`, not `fret-node` or `fret-core`.
