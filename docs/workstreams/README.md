# Legacy Workstream Evidence

This directory contains the pre-Trellis Jellyflow workstream system. These
workstreams are historical evidence, not active Trellis tasks.

## Migration Policy

- Keep closed workstreams in this directory so their design decisions, gates,
  evidence logs, and handoffs remain reviewable.
- Do not bulk-convert closed workstreams into `.trellis/tasks/`.
- New work should use Trellis tasks under `.trellis/tasks/`.
- New Trellis task artifacts should cite relevant legacy files when prior
  evidence constrains the scope:
  - `DESIGN.md` for design intent and tradeoffs;
  - `TODO.md` and `TASKS.jsonl` for executed task order;
  - `EVIDENCE_AND_GATES.md` for validation history;
  - `HANDOFF.md` for final state and follow-ons;
  - `WORKSTREAM.json` for machine-readable status, references, tags, and gates.

## Status Check

Use this command to inspect legacy workstream status:

```bash
for f in docs/workstreams/*/WORKSTREAM.json; do jq -r '[input_filename, .status, .title // .name // ""] | @tsv' "$f"; done
```

At Trellis bootstrap time, the visible `docs/workstreams/*/WORKSTREAM.json`
entries were closed. Future medium or cross-crate work should open a fresh
Trellis task and link the relevant closed workstream as evidence instead of
resuming the old lane in place.

## Source Priority

Accepted ADRs in `docs/adr/` outrank legacy workstream notes. `CONTEXT.md`
summarizes the current state, while legacy workstreams explain how the project
arrived there. If an old workstream appears to conflict with an accepted ADR or
current source code, treat it as historical context and plan an explicit update.
