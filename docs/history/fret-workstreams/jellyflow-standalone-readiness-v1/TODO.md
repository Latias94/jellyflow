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
- [ ] Treat `~/codes/rust/jellyflow` as the target local path for the future standalone repository,
      but do not create it until the history extraction slice is explicit.

## Tasks

- [x] JSR-010 Build the standalone extraction inventory.
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
  - Fresh evidence:
    - `docs/workstreams/jellyflow-standalone-readiness-v1/JSR-010_EXTRACTION_INVENTORY_2026-05-30.md`
  - Fresh gates:
    - `python3 tools/audit_crate.py --crate jellyflow-core`: passed.
    - `python3 tools/audit_crate.py --crate jellyflow-runtime`: passed.
    - `python3 tools/audit_crate.py --crate fret-node`: passed.
    - `cargo tree -p jellyflow-core --depth 2`: passed.
    - `cargo tree -p jellyflow-runtime --depth 2`: passed.
    - `cargo tree -p fret-node --no-default-features --features headless --depth 2`: passed.
    - `cargo check -p jellyflow-core`: passed.
    - `cargo check -p jellyflow-runtime`: passed.
    - `cargo check -p fret-node --all-features --tests`: passed.
    - `python3 tools/check_layering.py`: passed.
    - `cargo fmt --check`: passed.
    - `jq empty docs/workstreams/jellyflow-standalone-readiness-v1/WORKSTREAM.json`: passed.
    - `python3 tools/check_workstream_catalog.py`: passed.
    - `git diff --check`: passed.

- [x] JSR-015 Remove or consciously replace the `fret-core` dependency before external smoke.
  - Scope:
    - replace `fret_core::Modifiers` in `jellyflow-core`
    - replace public `fret_core::KeyCode` usage in `jellyflow-runtime`
    - replace public `fret_core::Rect` usage in fit-view helpers
    - keep Fret-specific conversions in `fret-node`
    - update manifest source-policy tests so standalone Jellyflow does not depend on Fret crates
  - Validation:
    - `cargo check -p jellyflow-core`
    - `cargo check -p jellyflow-runtime`
    - `cargo check -p fret-node --no-default-features --features headless --tests`
    - `cargo check -p fret-node --all-features --tests`
    - `python3 tools/check_layering.py`
  - Exit note: makes JSR-020 a true external-consumer smoke instead of a Fret-core path dependency
    smoke.
  - Fresh evidence:
    - `docs/workstreams/jellyflow-standalone-readiness-v1/JSR-015_FRET_CORE_DETACHMENT_2026-05-30.md`
  - Fresh gates:
    - `cargo check -p jellyflow-core`: passed.
    - `cargo check -p jellyflow-runtime`: passed.
    - `cargo check -p fret-node --no-default-features --features headless --tests`: passed.
    - `cargo check -p fret-node --all-features --tests`: passed.
    - `cargo nextest run -p jellyflow-core`: passed with 48 tests.
    - `cargo nextest run -p jellyflow-runtime`: passed with 67 tests.
    - `cargo nextest run -p fret-node --no-default-features`: passed with 24 tests.
    - `cargo nextest run -p fret-node --all-features`: passed with 371 tests.
    - `cargo clippy -p jellyflow-core --all-targets -- -D warnings`: passed.
    - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
    - `cargo tree -p jellyflow-core --depth 2`: passed; no Fret crates.
    - `cargo tree -p jellyflow-runtime --depth 2`: passed; no Fret crates.
    - `python3 tools/check_layering.py`: passed.
    - `cargo fmt --check`: passed.
    - `cargo metadata --format-version 1 --no-deps | jq ...`: passed.
    - `jq empty docs/workstreams/jellyflow-standalone-readiness-v1/WORKSTREAM.json`: passed.
    - `python3 tools/check_workstream_catalog.py`: passed.
    - `git diff --check`: passed.

- [x] JSR-020 Prove an external headless consumer smoke.
  - Scope:
    - create a temporary external path-dependency smoke outside the Fret workspace or record a
      repeatable local fixture
    - use `jellyflow-core` and `jellyflow-runtime` without depending on `fret-node` or `fret-core`
    - cover the smallest useful flow: graph construction, transaction apply, runtime store/update,
      and fit-view or viewport payloads
  - Validation:
    - external smoke `cargo check`
    - focused `cargo check -p jellyflow-core`
    - focused `cargo check -p jellyflow-runtime`
  - Exit note: proves standalone consumers do not need Fret UI or adapter crates.
  - Fresh evidence:
    - `tools/check_jellyflow_external_smoke.py`
    - `docs/workstreams/jellyflow-standalone-readiness-v1/JSR-020_EXTERNAL_SMOKE_2026-05-30.md`
  - Fresh gates:
    - `python3 tools/check_jellyflow_external_smoke.py`: passed; external temp consumer checked and
      `cargo tree` contained no `fret` or `fret-*` packages.
    - `python3 -m py_compile tools/check_jellyflow_external_smoke.py`: passed.
    - `cargo check -p jellyflow-core`: passed.
    - `cargo check -p jellyflow-runtime`: passed.
    - `cargo check -p fret-node --all-features --tests`: passed.
    - `python3 tools/check_layering.py`: passed.
    - `jq empty docs/workstreams/jellyflow-standalone-readiness-v1/WORKSTREAM.json`: passed.
    - `python3 tools/check_workstream_catalog.py`: passed.
    - `git diff --check`: passed.
    - `cargo fmt --check`: passed.

- [x] JSR-030 Decide the repository and publishing policy.
  - Scope:
    - compare independent repository, generated mirror, and delayed extraction options
    - record package names, versioning expectations, release ownership, CI gates, and docs scope
    - decide whether publishing should wait for API stabilization or can start with clear
      pre-1.0 compatibility notes
  - Validation:
    - readiness note reviewed against `docs/adr/0331-jellyflow-headless-node-graph-engine-boundary.md`
    - `cargo publish --dry-run` only if package metadata is intentionally prepared in this lane
  - Exit note: gives a concrete next execution lane instead of mixing policy with extraction.
  - Fresh evidence:
    - `docs/workstreams/jellyflow-standalone-readiness-v1/JSR-030_REPOSITORY_PUBLISHING_POLICY_2026-05-30.md`
  - Fresh gates:
    - `cargo metadata --format-version 1 --no-deps | jq ...`: passed.
    - `cargo search jellyflow --limit 10`: passed; no results returned on 2026-05-30.
    - `cargo search jellyflow-core --limit 10`: passed; no results returned on 2026-05-30.
    - `cargo search jellyflow-runtime --limit 10`: passed; no results returned on 2026-05-30.
    - `jq empty docs/workstreams/jellyflow-standalone-readiness-v1/WORKSTREAM.json`: passed.
    - `python3 tools/check_workstream_catalog.py`: passed.
    - `git diff --check`: passed.
    - `cargo publish --dry-run`: not run because this policy slice intentionally does not prepare
      standalone package metadata.

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
