# Jellyflow Delete Contract v1 - Milestones

Status: Active
Last updated: 2026-06-02

## M0 - Scope And Evidence Freeze

Exit criteria:

- workstream artifacts agree on target state and first executable task;
- source coverage names XyFlow delete evidence, ADRs, prior model-policy follow-on, and current
  Jellyflow delete code;
- `CONTEXT.md` points at this active workstream.

Status: complete on 2026-06-02. JDC-020 is the first executable task.

## M1 - Template Delete Smoke

Exit criteria:

- headless adapter template built-in suite includes a delete selection scenario;
- scenario uses high-level conformance action vocabulary, not raw graph transactions;
- expected trace includes delete commit, XyFlow-style delete callbacks, and selection cleanup;
- focused conformance/template gates pass.

Status: pending.

## M2 - Documentation And Closeout

Exit criteria:

- README/runtime README document `runtime::delete` and `runtime::keyboard` responsibilities;
- `CONTEXT.md` no longer presents delete planner ownership as an unresolved follow-on;
- closeout audit records follow-ons such as async pre-delete or renderer confirmation dialogs;
- package, clippy, JSON, and diff gates pass.

Status: pending.
