# Jellyflow Adapter Template v1 - Evidence And Gates

Status: Active
Last updated: 2026-06-02

## Required Gates

- `cargo test --manifest-path templates/headless-adapter/Cargo.toml`
- `cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check`
- `python3 tools/check_external_consumer_smoke.py`
- `python3 tools/check_no_fret_dependencies.py`
- `cargo fmt --check`
- `jq empty docs/workstreams/jellyflow-adapter-template-v1/WORKSTREAM.json docs/workstreams/jellyflow-adapter-template-v1/TASKS.jsonl docs/workstreams/jellyflow-adapter-template-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-adapter-template-v1/CONTEXT.jsonl`
- `git diff --check`

## Evidence Log

### 2026-06-02 - JAT-010 Scope Freeze

Claim: The adapter template lane is open and ready for the first implementation task.

Evidence:

- Workstream docs define the target template boundary.
- ADR 0003 remains the renderer boundary authority.
- `WORKSTREAM.json` points `current_task` at `JAT-020`.

Commands:

- `jq empty docs/workstreams/jellyflow-adapter-template-v1/WORKSTREAM.json docs/workstreams/jellyflow-adapter-template-v1/TASKS.jsonl docs/workstreams/jellyflow-adapter-template-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-adapter-template-v1/CONTEXT.jsonl`: passed.

### 2026-06-02 - JAT-020 Headless Template

Claim: The non-workspace adapter template runs a built-in headless conformance suite.

Evidence:

- `templates/headless-adapter` contains a copyable crate with path dependencies on
  `jellyflow-core` and `jellyflow-runtime`.
- The built-in suite covers node drag and viewport pan callback ordering.
- The CLI supports built-in checks and fixture-directory check/approve flows.

Commands:

- `cargo test --manifest-path templates/headless-adapter/Cargo.toml`: passed with 5 tests.
- `cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check`: passed and printed a
  matching suite report.

### 2026-06-02 - JAT-030 Smoke And Docs

Claim: External smoke and docs now cover the adapter template.

Evidence:

- `tools/check_external_consumer_smoke.py` runs the temporary external crate and the
  `templates/headless-adapter` manifest.
- Root and runtime READMEs show the template commands.
- The template cargo tree is checked for Fret packages by external smoke.

Commands:

- `cargo fmt --check`: passed.
- `cargo fmt --manifest-path templates/headless-adapter/Cargo.toml --check`: passed.
- `python3 -m py_compile tools/check_external_consumer_smoke.py`: passed.
- `python3 tools/check_external_consumer_smoke.py`: passed for the temporary project and template.
- `python3 tools/check_no_fret_dependencies.py`: passed.
- `git diff --check`: passed.

## Review Notes

- The template must remain outside the workspace member list.
- The template must not add dependencies from `jellyflow-core` or `jellyflow-runtime` back to any
  renderer or platform crate.
- Renderer smoke tests belong to future adapter-specific lanes.
