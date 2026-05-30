# Jellyflow Standalone Readiness v1 - Handoff

Status: Closed
Last updated: 2026-05-30

## Current State

This lane is closed as a follow-on to `jellyflow-package-split-v1`. The package split lane proved the
in-workspace `jellyflow-core` and `jellyflow-runtime` boundaries, then closed without moving code to
a separate repository, publishing crates, removing compatibility re-exports, or extracting geometry.

JSR-010, JSR-015, JSR-020, JSR-030, and JSR-040 are complete. The readiness closeout recommends a
new standalone repository with history-preserving extraction as the next lane and keeps crates.io
publishing blocked until standalone metadata, docs, CI, release-plz, and dry-run gates exist.

## Assumptions

- Confident: package dependencies, metadata gaps, documentation gaps, compatibility policy, and
  external-consumer smoke are explicit enough to start the extraction lane.
- Confident: `fret-node` remains the Fret adapter and compatibility facade.
- Confident: geometry stays out of scope because the package split lane found it is still
  adapter-bound.
- Confident: the preferred future local repo path is `~/codes/rust/jellyflow`.
- Confident: the next lane should use a fresh Fret clone and history-preserving Git extraction.

## Next Task

Open `jellyflow-repo-extraction-v1`.

The follow-on should create `~/codes/rust/jellyflow` from a fresh Fret clone with filtered history,
then bootstrap standalone metadata, docs, CI, package dry-runs, and Fret adapter dependency policy.
