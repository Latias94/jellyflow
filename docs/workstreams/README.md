# Removed Legacy Workstreams

The pre-Trellis Jellyflow workstream lane directories were removed after the
Trellis bootstrap. New work should use Trellis tasks under `.trellis/tasks/`.

## Current Policy

- Do not recreate the old `WORKSTREAM.json` / `TODO.md` / `EVIDENCE_AND_GATES.md`
  lane system for new work.
- Use `.trellis/tasks/` for new requirements, planning, execution state, and
  archived task records.
- Use `.trellis/spec/` for durable coding conventions and architecture guardrails
  that future agents must load.
- Use accepted ADRs in `docs/adr/` for architecture decisions.
- Use `CONTEXT.md` as the high-signal navigation summary.

## Preserved History

Extraction-era workstream history that still supports ADR 0001 remains under
`docs/history/fret-workstreams/`.

The field taxonomy that supported ADR 0002 was promoted out of the deleted
workstream tree into:

```text
docs/adr/0002-field-taxonomy-2026-05-30.md
```

All other removed `docs/workstreams/jellyflow-*` lane files remain recoverable
from git history if a future archaeology task needs them.
