# Jellyflow Model Policy Boundary v1 - Milestones

Status: Closed
Last updated: 2026-05-30

## M0 - Scope And Evidence Freeze

Exit criteria:

- Follow-on scope is explicit.
- Non-goals prevent premature schema migration.
- First executable task is selected.

Primary evidence:

- `docs/workstreams/jellyflow-model-policy-boundary-v1/DESIGN.md`
- `docs/workstreams/jellyflow-model-policy-boundary-v1/TODO.md`

## M1 - Taxonomy And Decision Record

Exit criteria:

- Existing fields are classified by ownership and lifecycle.
- Any hard-to-reverse schema movement is gated by ADR.
- The lane knows whether v1 is additive-only or migration-bearing.

Primary gates:

- `cargo fmt --check`
- `git diff --check`

## M2 - Policy Resolution Facade

Exit criteria:

- Runtime has pure policy-resolution helpers.
- Tests prove global default and per-element override precedence.
- Touched behavior paths use one shared precedence contract.

Primary gates:

- `cargo check -p jellyflow-runtime`
- `cargo nextest run -p jellyflow-runtime policy`
- `cargo nextest run -p jellyflow-runtime rules`
- `cargo nextest run -p jellyflow-runtime runtime`

## M3 - Compatibility And Closeout

Exit criteria:

- XyFlow compatibility remains explicit and tested.
- Canonical docs describe Jellyflow policy terms.
- Final gates are recorded.
- Remaining schema migration or geometry work is split into follow-ons.

Primary gates:

- `cargo fmt --check`
- `cargo nextest run --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `python3 tools/check_no_fret_dependencies.py`
- `python3 tools/check_external_consumer_smoke.py`

Exit status: met on 2026-05-30. Final evidence is recorded in `EVIDENCE_AND_GATES.md` and
`CLOSEOUT_AUDIT_2026-05-30.md`.
