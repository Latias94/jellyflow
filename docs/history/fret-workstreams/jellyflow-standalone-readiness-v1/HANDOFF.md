# Jellyflow Standalone Readiness v1 - Handoff

Status: Active
Last updated: 2026-05-30

## Current State

This lane is open as a follow-on to `jellyflow-package-split-v1`. The package split lane proved the
in-workspace `jellyflow-core` and `jellyflow-runtime` boundaries, then closed without moving code to
a separate repository, publishing crates, removing compatibility re-exports, or extracting geometry.

## Assumptions

- Confident: standalone repository extraction should not start until package dependencies,
  metadata, documentation, compatibility policy, and external-consumer smoke are explicit.
- Confident: `fret-node` remains the Fret adapter and compatibility facade during this lane.
- Confident: geometry stays out of scope because the previous lane found it is still adapter-bound.
- Likely: the first useful step is an extraction inventory, not a repository move.

## Next Task

Start with JSR-010 in `TODO.md`: build the standalone extraction inventory.

The first pass should inspect package metadata, direct dependencies, workspace-only assumptions,
documentation gaps, release tooling expectations, and `fret-node` compatibility promises. It should
end with concrete go/no-go criteria for external smoke and publishing readiness.
