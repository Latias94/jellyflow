# Jellyflow Adapter Template v1 - Milestones

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

- JAT-010 is complete when workstream artifacts agree on problem, renderer boundary, target state,
  current task, context manifest, and gates.

## M1 - Copyable Headless Adapter Template

- A non-workspace `templates/headless-adapter` crate exists.
- The template uses public `jellyflow-core` and `jellyflow-runtime` APIs through path dependencies.
- The template has a headless conformance suite and at least one test or CLI gate that runs it.
- The template has no renderer, platform, Fret, screenshot, or pixel dependencies.

## M2 - Template Smoke Integration And Docs

- External smoke tooling runs the template as an external consumer.
- No-Fret tooling covers the template dependency tree or equivalent cargo tree evidence is recorded.
- Root/runtime docs point adapter authors at the template and keep renderer smoke tests outside
  `jellyflow-core` and `jellyflow-runtime`.

## M3 - Closeout

- Final gate evidence is recorded in `EVIDENCE_AND_GATES.md`.
- Workstream status is closed only after template cargo gates, external smoke, no-Fret checks, JSON
  validation, formatting, and diff checks pass.
- Remaining renderer-specific adapter work is split or explicitly deferred.
