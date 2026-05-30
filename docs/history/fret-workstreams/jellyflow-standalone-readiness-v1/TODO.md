# Jellyflow Standalone Readiness v1 - TODO

Status: Active

## Guardrails

- [x] Do not reopen `jellyflow-package-split-v1`; this is a narrow follow-on lane.
- [ ] Do not move code into a separate repository from this lane.
- [ ] Do not publish crates from this lane.
- [ ] Keep `fret-node` as the Fret adapter and compatibility facade.
- [ ] Keep Fret UI, overlays, portals, kit profiles, and renderer/platform behavior out of
      standalone Jellyflow readiness work.
- [ ] Do not extract geometry until there is a cleaner reusable seam or a second consumer.

## Tasks

- [ ] JSR-010 Build the standalone extraction inventory.
  - Scope:
    - audit `jellyflow-core`, `jellyflow-runtime`, and `fret-node` package metadata
    - audit direct dependencies and workspace-only assumptions
    - list missing README/API docs/examples/license/version/release-plz requirements
    - identify compatibility re-export promises that a standalone package must preserve or
      intentionally deprecate
  - Validation:
    - `cargo check -p jellyflow-core`
    - `cargo check -p jellyflow-runtime`
    - `cargo check -p fret-node --all-features --tests`
    - `python3 tools/check_layering.py`
  - Exit note: produces the first extraction-readiness inventory without changing package layout.

- [ ] JSR-020 Prove an external headless consumer smoke.
  - Scope:
    - create a temporary external path-dependency smoke outside the Fret workspace or record a
      repeatable local fixture
    - use `jellyflow-core` and `jellyflow-runtime` without depending on `fret-node`
    - cover the smallest useful flow: graph construction, transaction apply, runtime store/update,
      and fit-view or viewport payloads
  - Validation:
    - external smoke `cargo check`
    - focused `cargo check -p jellyflow-core`
    - focused `cargo check -p jellyflow-runtime`
  - Exit note: proves standalone consumers do not need Fret UI or adapter crates.

- [ ] JSR-030 Decide the repository and publishing policy.
  - Scope:
    - compare independent repository, generated mirror, and delayed extraction options
    - record package names, versioning expectations, release ownership, CI gates, and docs scope
    - decide whether publishing should wait for API stabilization or can start with clear
      pre-1.0 compatibility notes
  - Validation:
    - readiness note reviewed against `docs/adr/0331-jellyflow-headless-node-graph-engine-boundary.md`
    - `cargo publish --dry-run` only if package metadata is intentionally prepared in this lane
  - Exit note: gives a concrete next execution lane instead of mixing policy with extraction.

- [ ] JSR-040 Close the readiness lane with a go/no-go packet.
  - Scope:
    - summarize passed gates, remaining gaps, and the recommended next lane
    - update `WORKSTREAM.json` to closed with `start_follow_on`
    - update the workstream catalog
  - Validation:
    - `jq empty docs/workstreams/jellyflow-standalone-readiness-v1/WORKSTREAM.json`
    - `python3 tools/check_workstream_catalog.py`
    - `git diff --check`
  - Exit note: closes the readiness work before any standalone repository move begins.
