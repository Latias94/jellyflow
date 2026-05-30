# Jellyflow Standalone Readiness v1 - Milestones

Status: Active
Last updated: 2026-05-30

## Milestones

- [x] M0 - Source boundary inherited.
  - `jellyflow-package-split-v1` closed after extracting `jellyflow-core` and
    `jellyflow-runtime` in-workspace.
  - Geometry extraction was explicitly deferred.

- [x] M1 - Extraction inventory complete.
  - JSR-010 records package metadata, dependency, documentation, release, and compatibility gaps.

- [x] M1.5 - Fret-core dependency resolved.
  - JSR-015 removes or consciously replaces the remaining `fret-core` input/geometry types before
    the external smoke is treated as standalone proof.

- [ ] M2 - External headless smoke complete.
  - JSR-020 proves a non-Fret consumer can use `jellyflow-core` and `jellyflow-runtime` through
    path dependencies.

- [ ] M3 - Repository and publish policy decided.
  - JSR-030 records whether the next execution lane should create a standalone repository, mirror
    the in-tree crates, publish from the monorepo, or delay extraction.

- [ ] M4 - Readiness closeout complete.
  - JSR-040 closes this lane with a go/no-go packet and explicit follow-on recommendation.
