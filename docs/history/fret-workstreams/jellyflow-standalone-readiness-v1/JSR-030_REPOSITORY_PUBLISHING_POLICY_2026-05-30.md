# JSR-030 Repository and Publishing Policy

Date: 2026-05-30
Status: Complete

## Decision

Use a new standalone repository as the target architecture for Jellyflow, with history-preserving
extraction from Fret into `~/codes/rust/jellyflow` as the next execution lane.

Do not publish Jellyflow crates from this readiness lane. Publishing should wait until the
standalone repository owns its package metadata, crate READMEs, CI, release configuration, and
`cargo publish --dry-run` evidence.

## Inputs

- `docs/adr/0001-jellyflow-headless-node-graph-engine-boundary.md`
- `docs/workstreams/jellyflow-standalone-readiness-v1/JSR-010_EXTRACTION_INVENTORY_2026-05-30.md`
- `docs/workstreams/jellyflow-standalone-readiness-v1/JSR-015_FRET_CORE_DETACHMENT_2026-05-30.md`
- `docs/workstreams/jellyflow-standalone-readiness-v1/JSR-020_EXTERNAL_SMOKE_2026-05-30.md`
- `tools/check_jellyflow_external_smoke.py`

## Options Compared

| Option | Strengths | Costs / Risks | Verdict |
| --- | --- | --- | --- |
| New independent repository | Clearest boundary for headless consumers; matches the user's preferred `~/codes/rust/jellyflow` target; allows Jellyflow-specific CI, docs, release cadence, and issue taxonomy; can preserve relevant authorship/history with path-filtered extraction. | Requires extraction script, new repo bootstrap, release-plz setup, cross-repo coordination with Fret, and a compatibility plan for `fret-node`. | Recommended. |
| Generated mirror from Fret | Keeps one source-of-truth repo while presenting a separate downstream repo; avoids immediate cross-repo dependency churn. | Blurs ownership, makes external contributions awkward, creates generated commits or mirror drift, and delays real boundary pressure. | Keep only as a fallback if standalone repo maintenance capacity is not available. |
| Delayed extraction | Lowest immediate cost; keeps all refactors atomic inside Fret. | Increases risk of Fret-specific dependency drift, weakens headless adoption story, and postpones the boundary clarity JSR-020 already made testable. | Not recommended after JSR-020 unless the next repo lane is intentionally paused. |

## Package Policy

Initial standalone package set:

- `jellyflow-core`: graph document model, IDs, type descriptors, interaction value types, ops,
  history, fragments, and diffs.
- `jellyflow-runtime`: headless I/O/view-state payloads, rules, schema/profile pipeline,
  store/apply/callback/controlled-mode helpers, and fit-view math.

Keep out of the initial standalone package set:

- `fret-node`: remains in Fret as the adapter and compatibility facade.
- `jellyflow-geometry`: deferred until geometry/spatial/path/hit-test contracts have a cleaner
  reusable boundary or a second non-Fret consumer.
- Fret UI, overlays, portals, renderer/platform integration, kit profiles, and app bindings.

Potential future packages:

- `jellyflow`: optional facade once the two-crate API settles.
- `jellyflow-fret`: only if the Fret adapter is intentionally moved out of the Fret monorepo later.

Crate name check on 2026-05-30:

- `cargo search jellyflow --limit 10`: no results returned.
- `cargo search jellyflow-core --limit 10`: no results returned.
- `cargo search jellyflow-runtime --limit 10`: no results returned.

This is only a current availability signal. Re-check names immediately before the first
`cargo publish` because crates.io name availability can change.

## Versioning Policy

Use lockstep `0.1.x` releases for `jellyflow-core` and `jellyflow-runtime` during the initial
standalone phase.

Pre-1.0 compatibility rule:

- breaking public API changes bump the minor version, for example `0.1` to `0.2`;
- compatible fixes and internal refactors bump the patch version;
- `jellyflow-runtime` depends on the matching compatible `jellyflow-core` line;
- `fret-node` keeps its Fret workspace version and updates its Jellyflow dependency through a
  deliberate adapter-compatibility PR.

This keeps early API movement honest without pretending the graph/runtime surface is stable enough
for a `1.0` contract.

## Release Ownership

Jellyflow standalone repository owns:

- `jellyflow-core` and `jellyflow-runtime` package metadata,
- package READMEs and examples,
- release-plz configuration for Jellyflow packages only,
- CI for headless gates,
- crates.io publishing decisions.

Fret repository owns:

- `fret-node`,
- Fret UI adapter behavior,
- compatibility re-exports for existing Fret users,
- Fret demo and diagnostics integration,
- updates that move `fret-node` from path dependencies to published or git Jellyflow dependencies.

## Required Standalone Repository Bootstrap

The next execution lane should create the repo from a fresh clone of Fret, not from the active
working directory.

Recommended history-preserving approach:

1. Create a fresh Fret clone.
2. Run `git filter-repo --analyze`.
3. Use a checked-in paths file for:
   - `ecosystem/jellyflow-core/`
   - `ecosystem/jellyflow-runtime/`
   - historical source paths under `ecosystem/fret-node/src/{core,types,interaction,ops,io,profile,rules,schema,runtime}/`
   - ADR/workstream evidence that should travel with the extracted history.
4. Apply explicit `--path-rename` rules only after the final standalone layout is chosen.
5. Record source repo, source commit, paths file, path-renames, and filter command in the new repo.
6. Validate history with `git log --follow` and `git blame` on representative files.
7. Add standalone root `Cargo.toml`, crate READMEs, licenses, CI, and release-plz config.

Prefer `git filter-repo` for this extraction because the path set spans multiple current and
historical locations. `git subtree split` is a possible fallback for simpler single-prefix history,
but it is a weaker fit here. Do not use `git filter-branch` for the new lane.

## CI and Gate Policy

Minimum standalone repo gates:

- `cargo fmt --check`
- `cargo check --workspace`
- `cargo nextest run --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo tree -p jellyflow-core --depth 2`
- `cargo tree -p jellyflow-runtime --depth 2`
- no `fret` / `fret-*` package in the headless dependency tree
- external consumer smoke adapted from `tools/check_jellyflow_external_smoke.py`
- `cargo package -p jellyflow-core --list`
- `cargo package -p jellyflow-runtime --list`
- `cargo publish --dry-run -p jellyflow-core`
- `cargo publish --dry-run -p jellyflow-runtime`

The dry-run gates belong to the standalone bootstrap/publish-readiness lane, not this JSR-030
policy slice, because the current crates still inherit Fret repository/homepage/documentation
metadata.

## Documentation Scope

Carry or recreate in the standalone repo:

- root `README.md` explaining Jellyflow as a headless node/flow graph engine,
- crate README for `jellyflow-core`,
- crate README for `jellyflow-runtime`,
- package-level examples for graph construction, transaction apply, store dispatch, schema/rules,
  and fit-view,
- boundary note derived from ADR 0001,
- migration note explaining that `fret-node` remains the Fret adapter.

Keep Fret-specific UI docs in the Fret repo and link to Jellyflow docs where the headless model is
shared.

## Go / No-Go

Go for a new standalone repository extraction lane.

No-go for crates.io publishing from the current Fret monorepo state. The package names look
unclaimed from `cargo search` on 2026-05-30, but publish readiness still requires metadata cleanup,
crate READMEs, standalone CI, release-plz setup, dry-runs, and a final name re-check.

## Recommended Follow-On Lane

Start `jellyflow-repo-extraction-v1` after JSR-040 closes this readiness lane.

Suggested first tasks:

1. Write the extraction paths file and path-rename map.
2. Create `~/codes/rust/jellyflow` from a fresh Fret clone with filtered history.
3. Bootstrap standalone workspace metadata, licenses, READMEs, and CI.
4. Port the external consumer smoke into the new repo.
5. Run package/dry-run gates without publishing.
6. Update Fret `fret-node` integration policy after the standalone repo is proven.

## Fresh Gates

- `cargo metadata --format-version 1 --no-deps | jq ...`: passed; confirms Jellyflow package
  metadata still points to Fret and publish dry-runs should wait.
- `cargo search jellyflow --limit 10`: passed; no results returned on 2026-05-30.
- `cargo search jellyflow-core --limit 10`: passed; no results returned on 2026-05-30.
- `cargo search jellyflow-runtime --limit 10`: passed; no results returned on 2026-05-30.
- `jq empty docs/workstreams/jellyflow-standalone-readiness-v1/WORKSTREAM.json`: passed.
- `python3 tools/check_workstream_catalog.py`: passed.
- `git diff --check`: passed.
