# Jellyflow Conformance Golden Approval v1 - Evidence And Gates

Status: Closed
Last updated: 2026-06-01

## Smallest Current Repro

Adapters can run and discover suite fixtures, but cannot use the runtime conformance API to
explicitly refresh golden `expected_trace` values from actual headless runtime behavior.

## Required Gates

- `cargo nextest run -p jellyflow-runtime conformance_approval`
- `cargo nextest run -p jellyflow-runtime --test public_surface`
- `cargo check -p jellyflow-runtime`
- `cargo fmt --check`
- `cargo nextest run -p jellyflow-runtime`
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
- `jq empty docs/workstreams/jellyflow-conformance-golden-approval-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-golden-approval-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-golden-approval-v1/CAMPAIGNS.jsonl`
- `git diff --check`

## Evidence Log

- 2026-06-01: JCGA-010 opened the golden approval lane.
  - Scope is limited to explicit headless approval/update primitives.
  - CLI commands, automatic approval, renderer assets, screenshots, and pixels remain out of scope.
- 2026-06-01: JCGA-020 added explicit headless golden approval/update primitives.
  - Added suite approval reports that replace `expected_trace` with actual runtime traces in an
    updated suite value.
  - Added file and directory `approve_actual_traces_to_json` helpers that write back JSON only when
    every scenario executes without errors.
  - Directory approval computes all approvals before writing files, preventing partial updates when
    execution errors are detected.
  - Reports and approval errors are serde-friendly for agent logs.
  - Tests cover suite approval, file write-back, successful directory write-back, and directory
    refusal without partial writes.
  - Public-surface smoke coverage uses suite, file, and directory approval APIs.
  - `cargo nextest run -p jellyflow-runtime conformance_approval`: passed, 4 tests.
    - Nextest run ID: `bcb774ad-9a63-4918-a68e-0afbbe60d78e`.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: passed, 3 tests.
    - Nextest run ID: `7f4dc4f6-bfe7-4f77-b9df-dcc6fbf06ffb`.
  - `cargo check -p jellyflow-runtime`: passed.
- 2026-06-01: JCGA-030 closed the golden approval workstream.
  - `cargo fmt --check`: passed.
  - `cargo nextest run -p jellyflow-runtime`: 177 passed, 0 skipped.
    - Nextest run ID: `a802ac75-57c9-489d-a0b9-aca931d733ff`.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: passed.
  - `jq empty docs/workstreams/jellyflow-conformance-golden-approval-v1/WORKSTREAM.json docs/workstreams/jellyflow-conformance-golden-approval-v1/TASKS.jsonl docs/workstreams/jellyflow-conformance-golden-approval-v1/CAMPAIGNS.jsonl`: passed.
  - `git diff --check`: passed.
  - Documentation: `README.md`, `crates/jellyflow-runtime/README.md`, and
    `CLOSEOUT_AUDIT_2026-06-01.md`.

## Notes

This workstream is closed. Follow-ons are split below in `HANDOFF.md` and the closeout audit.
