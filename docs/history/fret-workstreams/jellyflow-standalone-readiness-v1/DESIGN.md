# Jellyflow Standalone Readiness v1

Status: Active
Last updated: 2026-05-30

## Problem

`jellyflow-core` and `jellyflow-runtime` now exist inside the Fret monorepo, but that is not enough
to extract or publish Jellyflow as an independent package family. A standalone Jellyflow path needs
explicit evidence that the headless crates are usable without Fret UI, renderer, platform, runner,
or workspace-only assumptions.

## Target State

This lane produces a decision-ready extraction packet for Jellyflow:

1. Package inventory for `jellyflow-core`, `jellyflow-runtime`, and the `fret-node` adapter surface.
2. External-consumer smoke proof for headless path dependencies.
3. Publish-readiness notes for metadata, README/API docs, license, versioning, and release tooling.
4. Compatibility policy for the existing `fret-node` re-export facade.
5. A repository strategy recommendation for a future standalone Jellyflow repository or mirror.

## In Scope

- Auditing direct and workspace dependencies of the Jellyflow crates.
- Proving headless use from outside the Fret workspace.
- Recording package metadata, documentation, examples, and release gaps.
- Deciding whether standalone extraction should be a new repository, mirror, or delayed.
- Keeping the outcome narrow enough to close before any physical repository move.

## Out Of Scope

- Moving code to a separate repository.
- Publishing crates to crates.io.
- Removing `fret-node` compatibility re-exports.
- Creating `jellyflow-geometry`.
- Moving Fret UI, overlays, portals, kit profiles, or renderer-owned behavior into Jellyflow.

## First Slice

JSR-010 audits the current package surface and produces a concrete extraction inventory. It should
answer which dependencies, metadata fields, docs, examples, gates, and compatibility shims must be
fixed before Jellyflow can leave the Fret monorepo safely.
