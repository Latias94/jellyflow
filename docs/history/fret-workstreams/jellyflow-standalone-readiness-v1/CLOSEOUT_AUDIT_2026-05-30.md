# Jellyflow Standalone Readiness v1 Closeout Audit

Date: 2026-05-30
Status: Closed

## Verdict

Close this readiness lane.

Go for a follow-on `jellyflow-repo-extraction-v1` lane that creates a standalone repository at
`~/codes/rust/jellyflow` from a fresh Fret clone with history-preserving extraction.

No-go for crates.io publishing from the current Fret monorepo state. Publishing remains blocked on
standalone metadata, package READMEs, CI, release configuration, dry-run evidence, and a final
crates.io name availability re-check.

## Target State Check

| Target | Evidence | Result |
| --- | --- | --- |
| Package inventory for Jellyflow and `fret-node` adapter | `JSR-010_EXTRACTION_INVENTORY_2026-05-30.md` | Complete |
| No remaining `fret-core` dependency in headless Jellyflow crates | `JSR-015_FRET_CORE_DETACHMENT_2026-05-30.md` | Complete |
| External headless consumer proof | `JSR-020_EXTERNAL_SMOKE_2026-05-30.md` and `tools/check_jellyflow_external_smoke.py` | Complete |
| Repository and publishing policy | `JSR-030_REPOSITORY_PUBLISHING_POLICY_2026-05-30.md` | Complete |
| No physical repository move or publish from this lane | Git status and workstream scope | Preserved |

## What This Lane Proved

- `jellyflow-core` and `jellyflow-runtime` are clean enough for standalone-repository extraction
  planning.
- Headless Jellyflow no longer depends on `fret-core`, `fret-node`, Fret UI, renderer, platform,
  runner, `wgpu`, or `winit`.
- A temporary external Cargo consumer can path-depend only on `jellyflow-core` and
  `jellyflow-runtime`, compile, and verify that no `fret` / `fret-*` package appears in the
  dependency tree.
- The recommended repository policy is a new standalone repo with history-preserving extraction,
  not a generated mirror and not further delay.
- Publishing should wait for standalone repo bootstrap and dry-run evidence.

## Remaining Gaps

- `~/codes/rust/jellyflow` does not exist yet.
- `jellyflow-core` and `jellyflow-runtime` still inherit Fret repository/homepage/documentation
  metadata in this monorepo.
- Package READMEs, standalone root README, standalone CI, and Jellyflow release-plz configuration
  are not created yet.
- `cargo publish --dry-run` was intentionally not run in this lane.
- Geometry/spatial/path/hit-test extraction remains deferred.
- `fret-node` compatibility re-exports remain in Fret and should be handled by a separate adapter
  policy if the standalone repo changes public API shape.

## Follow-On

Start `jellyflow-repo-extraction-v1`.

Recommended first tasks:

1. Write the extraction paths file and path-rename map.
2. Create `~/codes/rust/jellyflow` from a fresh Fret clone using `git filter-repo`.
3. Record source commit, paths file, command line, and path-renames in the new repo.
4. Bootstrap standalone workspace metadata, license files, READMEs, examples, CI, and release-plz.
5. Port the external consumer smoke and no-Fret dependency-tree checks.
6. Run `cargo package --list` and `cargo publish --dry-run` without publishing.
7. Decide how Fret `fret-node` consumes the proven standalone Jellyflow packages.

## Closeout Gates

- `jq empty docs/workstreams/jellyflow-standalone-readiness-v1/WORKSTREAM.json`: passed.
- `python3 tools/check_workstream_catalog.py`: passed.
- `git diff --check`: passed.

Broader Rust checks were not rerun for JSR-040 because this is a docs/state closeout after JSR-020
and JSR-030 already recorded the current code and package-policy evidence.
