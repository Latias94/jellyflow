# Jellyflow Viewport Gesture Policy v1 - Milestones

Status: Active
Last updated: 2026-06-01

## M0 - Scope And Evidence Freeze

Exit criteria:

- `DESIGN.md`, `TODO.md`, `TASKS.jsonl`, `CAMPAIGNS.jsonl`, `CONTEXT.jsonl`,
  `EVIDENCE_AND_GATES.md`, `WORKSTREAM.json`, and `HANDOFF.md` agree.
- ADR-0001 and ADR-0003 constraints are referenced.
- First executable task is `JVGP-020`.

Status: complete on 2026-06-01.

## M1 - Headless Policy Proof

Exit criteria:

- A renderer-neutral viewport gesture policy module exists.
- Wheel/pinch and drag-pan gate decisions are covered by focused runtime tests.
- Existing viewport pan/zoom math behavior remains covered.
- `cargo fmt --check` and `cargo nextest run -p jellyflow-runtime viewport` pass.

Status: complete on 2026-06-01.

## M2 - Conformance Integration

Exit criteria:

- Adapter conformance scenarios can exercise policy decisions without renderer/platform events.
- Trace output can expose accepted/rejected viewport gesture intent where needed.
- `cargo nextest run -p jellyflow-runtime adapter_conformance` and
  `cargo nextest run -p jellyflow-runtime conformance` pass.

Status: complete on 2026-06-01.

## M3 - Public Surface And Closeout

Exit criteria:

- Public surface smoke covers exported policy types when they are public.
- Runtime package and clippy gates pass.
- Workstream evidence is current and machine-readable state is valid.
- Remaining XyFlow panzoom gaps are split or deferred.

Status: complete on 2026-06-01.
