# Jellyflow Geometry Spatial v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Evidence Freeze

Exit criteria:

- Problem and target state are explicit.
- Non-goals are explicit.
- Relevant ADRs, previous workstreams, XyFlow reference paths, and code evidence are linked.
- First proof target is chosen.

Primary evidence:

- `docs/workstreams/jellyflow-geometry-spatial-v1/DESIGN.md`
- `docs/workstreams/jellyflow-geometry-spatial-v1/TODO.md`
- `docs/workstreams/jellyflow-geometry-spatial-v1/CONTEXT.jsonl`

## M1 - Shared Geometry Proof

Exit criteria:

- A shared geometry module exists or an equivalent internal module structure is in place.
- Existing fit-view and runtime bounds helpers use the shared primitives.
- Public behavior is preserved unless an intentional API change is recorded.
- Follow-up scope is recorded instead of silently widened.

Primary gates:

- `cargo nextest run -p jellyflow-runtime runtime::fit_view`
- `cargo nextest run -p jellyflow-runtime runtime::tests::utils`
- `cargo check -p jellyflow-runtime`

## M2 - Derived Layout And Spatial Queries

Exit criteria:

- Lookup-derived geometry has a clear owner.
- Parent/child and visibility queries are deterministic.
- Spatial-index tuning either backs tested behavior or is explicitly split into a follow-on.
- Edge endpoint/path work has a stable, renderer-neutral contract or a named follow-on.

Primary gates:

- `cargo nextest run -p jellyflow-runtime runtime::tests::lookups`
- `cargo nextest run -p jellyflow-runtime runtime::tests::utils`
- targeted edge endpoint/path filters as tasks land

## M3 - Integration And Docs

Exit criteria:

- Public or internal module paths are documented.
- Examples and READMEs reflect the shipped behavior.
- Evidence logs name what each gate proves.

Primary gates:

- `cargo nextest run -p jellyflow-runtime`
- `cargo check -p jellyflow-runtime`

## M4 - Closeout

Exit criteria:

- Final gate set is recorded with fresh evidence.
- Remaining work is completed, deferred, or split into follow-ons.
- `WORKSTREAM.json` status is updated.
- Handoff describes the next safe continuation point.

Primary gates:

- `cargo fmt --check`
- `cargo nextest run --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `python3 tools/check_no_fret_dependencies.py`
- `python3 tools/check_external_consumer_smoke.py`
- `jq empty docs/workstreams/jellyflow-geometry-spatial-v1/WORKSTREAM.json`
- `git diff --check`
