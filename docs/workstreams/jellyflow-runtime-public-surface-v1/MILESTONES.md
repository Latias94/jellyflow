# Jellyflow Runtime Public Surface v1 - Milestones

Status: Closed
Last updated: 2026-05-30

## M0 - Scope And Evidence Freeze

Exit criteria:

- Problem and target state are explicit.
- Non-goals are explicit.
- ADR 0331 and closed extraction/readiness workstreams are linked.
- First executable task is chosen.

Primary evidence:

- `docs/workstreams/jellyflow-runtime-public-surface-v1/DESIGN.md`
- `docs/workstreams/jellyflow-runtime-public-surface-v1/TODO.md`

## M1 - Public Surface Shrink

Exit criteria:

- Runtime no longer exposes pass-through `core`, `interaction`, `ops`, or `types` modules.
- Internal runtime code imports core vocabulary from `jellyflow_core`.
- XyFlow compatibility has an explicit module home.
- Store dispatch remains behaviorally covered.

Primary gates:

- `cargo check -p jellyflow-runtime`
- `cargo nextest run -p jellyflow-runtime`
- `python3 tools/check_external_consumer_smoke.py`

## M2 - IO And Store Deepening

Exit criteria:

- IO/config/persistence/view-state/tuning responsibilities live in focused modules.
- No Fret-branded default path policy remains in Jellyflow runtime.
- `NodeGraphStore` public facade still works, but private implementation is split by concern.
- README/examples reflect the shipped public paths.

Primary gates:

- `cargo nextest run -p jellyflow-runtime io`
- `cargo nextest run -p jellyflow-runtime runtime`
- `cargo check -p jellyflow-runtime`
- `python3 tools/check_external_consumer_smoke.py`

## M3 - Closeout

Exit criteria:

- Final gate set is recorded.
- Workstream review has no blocking findings.
- Remaining work is completed, deferred, or split into follow-ons.
- `WORKSTREAM.json` and `HANDOFF.md` reflect the final state.

Primary gates:

- `cargo fmt --check`
- `cargo nextest run --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `python3 tools/check_no_fret_dependencies.py`
- `python3 tools/check_external_consumer_smoke.py`

Exit status: met on 2026-05-30. The lane is closed with final gates recorded in
`EVIDENCE_AND_GATES.md` and `CLOSEOUT_AUDIT_2026-05-30.md`.
