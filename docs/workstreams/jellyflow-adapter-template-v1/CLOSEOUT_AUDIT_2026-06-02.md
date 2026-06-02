# Jellyflow Adapter Template v1 - Closeout Audit

Date: 2026-06-02

## Final Status

Closed. JAT-010 through JAT-040 are complete.

## Completed Outcomes

- Opened a dedicated follow-on lane for adapter templates from the adapter conformance runner
  closeout.
- Added `templates/headless-adapter` as a non-workspace, external-consumer-style crate.
- Added built-in node drag and viewport pan conformance scenarios.
- Added template tests for built-in suite execution and fixture-directory roundtrip checks.
- Added a template CLI for built-in `check`, fixture-directory `check`, and fixture-directory
  `approve`.
- Extended external smoke to run the template through `cargo --manifest-path` and check its cargo
  tree for Fret packages.
- Updated root and runtime README material with template commands.
- Kept renderer, platform, Fret, screenshot, and pixel dependencies outside the headless crates.

## Review

`review-workstream` self-review found no blocking findings.

- Workstream compliance: all task ledger items are complete, target state is met, and ADR 0003's
  renderer-free boundary is preserved.
- Code quality: the template consumes public Jellyflow APIs only, stays outside the workspace
  member list, proves behavior through conformance reports, and keeps smoke output concise.
- Missing gates: none after closeout verification.
- Residual risk: the lane does not implement an actual renderer adapter or pixel/screenshot smoke
  tests. Those remain adapter-specific follow-ons.

REVIEW_RESULT: PASS

## Verification

`verify-rust-workstream` closeout claim: the adapter template lane is documented, complete, and
freshly verified.

- `cargo nextest run --workspace`: passed with 334 tests.
- `cargo test --manifest-path templates/headless-adapter/Cargo.toml`: passed with 5 tests.
- `cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check`: passed.
- `python3 tools/check_external_consumer_smoke.py`: passed for the temporary project and template.
- `python3 tools/check_no_fret_dependencies.py`: passed.
- `cargo fmt --check`: passed.
- `cargo fmt --manifest-path templates/headless-adapter/Cargo.toml --check`: passed.
- `jq empty docs/workstreams/jellyflow-adapter-template-v1/WORKSTREAM.json docs/workstreams/jellyflow-adapter-template-v1/TASKS.jsonl docs/workstreams/jellyflow-adapter-template-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-adapter-template-v1/CONTEXT.jsonl`: passed.
- `git diff --check`: passed.

VERIFY_RESULT: PASS

## Follow-Ons

- Renderer-specific smoke lanes for future `jellyflow-wgpu`, `jellyflow-egui`, or Fret adapters.
- Committed golden JSON fixture assets in downstream adapter repos when programmatic template
  scenarios are not enough.
- Broader gesture-family templates after parent expansion, double-click zoom, or pan inertia
  kernels exist.
