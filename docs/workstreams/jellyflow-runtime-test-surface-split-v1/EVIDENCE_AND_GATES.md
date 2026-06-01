# Jellyflow Runtime Test Surface Split v1 - Evidence And Gates

Status: Closed
Last updated: 2026-06-01

## Smallest Current Repro

The largest runtime maintenance files are test and harness files, including:

- `crates/jellyflow-runtime/src/runtime/tests/conformance.rs`
- `crates/jellyflow-runtime/src/runtime/tests/drag.rs`
- `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance.rs`
- `crates/jellyflow-runtime/src/runtime/tests/harness.rs`

These files mix scenario cases, setup, expected traces, helper assertions, and behavior-specific
fixtures. The problem is navigability and extension locality, not runtime behavior.

## Required Gates

- `cargo fmt --check`
- `cargo nextest run -p jellyflow-runtime conformance`
- `cargo nextest run -p jellyflow-runtime --example conformance_harness`
- `cargo nextest run -p jellyflow-runtime --test public_surface`
- `cargo nextest run -p jellyflow-runtime drag`
- `cargo nextest run -p jellyflow-runtime selection`
- `cargo nextest run -p jellyflow-runtime viewport`
- `cargo nextest run -p jellyflow-runtime`
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`
- `jq empty docs/workstreams/jellyflow-runtime-test-surface-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-runtime-test-surface-split-v1/TASKS.jsonl docs/workstreams/jellyflow-runtime-test-surface-split-v1/CAMPAIGNS.jsonl`
- `git diff --check`

## Evidence Log

- 2026-06-01: JRTSS-010 opened the runtime test-surface split lane.
  - Scope is a behavior-preserving, test-only refactor under `jellyflow-runtime`.
  - Public API paths, fixture JSON schema, conformance trace behavior, production runtime behavior,
    and renderer-free boundaries must remain unchanged.
- 2026-06-01: JRTSS-020 split the runtime conformance, adapter-conformance, and harness test
  surface.
  - `crates/jellyflow-runtime/src/runtime/tests/conformance/mod.rs` is now a facade over
    `approval`, `file_io`, `runner`, and `support`.
  - `crates/jellyflow-runtime/src/runtime/tests/adapter_conformance/mod.rs` is now a facade over
    `fixture_runner`, `geometry`, `projections`, and `support`.
  - `crates/jellyflow-runtime/src/runtime/tests/harness/mod.rs` is now a facade over `events`,
    `interaction`, and `recorder`.
  - The split is test-only and does not change production runtime modules, public APIs, fixture
    JSON schema, conformance trace semantics, callback payloads, or renderer boundaries.
  - `cargo fmt --check`: pass.
  - `cargo nextest run -p jellyflow-runtime conformance`: pass, 26 tests, run ID
    `adc09f96-780f-4864-ba64-a9678abd8fd5`.
  - `cargo nextest run -p jellyflow-runtime --example conformance_harness`: pass, 3 tests, run ID
    `2cf73a21-10e4-45a4-8de5-6a37cb425037`.
  - `cargo nextest run -p jellyflow-runtime --test public_surface`: pass, 3 tests, run ID
    `7daab3e2-e73c-4145-bd15-433e1931495f`.
  - `cargo nextest run -p jellyflow-runtime`: pass, 177 tests, run ID
    `1e83d140-e309-4ab7-af26-986c2bcf8717`.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: pass.
  - Review result: pass. No blocking workstream-compliance or code-quality findings; the diff is
    test-only and stays within JRTSS-020 scope.
  - Verification result: pass. Required task gates plus package and clippy gates passed. Workspace
    nextest was not run because the change is confined to `jellyflow-runtime` tests and the package
    gate covers the touched crate.
- 2026-06-01: JRTSS-030 split the drag and selection runtime test surface.
  - `crates/jellyflow-runtime/src/runtime/tests/drag/mod.rs` is now a facade over `single`,
    `multi`, and `support`.
  - `crates/jellyflow-runtime/src/runtime/tests/selection/mod.rs` is now a facade over
    `box_selection` and `support`.
  - `viewport.rs` and `auto_pan.rs` remain compact direct test modules; they were validated but
    not split because their current size and ownership are still clear.
  - The split is test-only and does not change node drag, selection, viewport, auto-pan, store
    dispatch, callback behavior, production runtime modules, public APIs, or renderer boundaries.
  - `cargo fmt --check`: pass.
  - `cargo nextest run -p jellyflow-runtime drag`: pass, 10 tests, run ID
    `5cf3097f-e382-4a55-b233-41b47c2a19cf`.
  - `cargo nextest run -p jellyflow-runtime selection`: pass, 9 tests, run ID
    `ac9b60e3-c948-4233-adf7-d8c949cf7adb`.
  - `cargo nextest run -p jellyflow-runtime viewport`: pass, 16 tests, run ID
    `ebb28395-3dfe-4fd6-b803-7887d3e55e8f`.
  - `cargo nextest run -p jellyflow-runtime`: pass, 177 tests, run ID
    `18aa2bf9-8b40-4b0f-88e0-d767cac457de`.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: pass.
  - Review result: pass. No blocking workstream-compliance or code-quality findings; the split is
    test-only and stays within JRTSS-030 scope.
  - Verification result: pass. Required task gates plus package and clippy gates passed.
- 2026-06-01: JRTSS-040 closed the workstream after final review and verification.
  - `cargo fmt --check`: pass.
  - `cargo nextest run -p jellyflow-runtime`: pass, 177 tests, run ID
    `13d5aa96-cb6e-46ee-a737-66f67c007bc3`.
  - `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: pass.
  - `jq empty docs/workstreams/jellyflow-runtime-test-surface-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-runtime-test-surface-split-v1/TASKS.jsonl docs/workstreams/jellyflow-runtime-test-surface-split-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-runtime-test-surface-split-v1/CONTEXT.jsonl`: pass.
  - `git diff --check`: pass.
  - Review result: pass. The lane target state is met, task ledger is complete, final docs are
    consistent, and no scope creep into production code, core tests, new gesture behavior, adapters,
    or renderer dependencies occurred.
  - Verification result: pass. Fresh closeout gates prove the touched runtime package and
    workstream artifacts are valid.

## Review Gate

Run `review-workstream` before accepting each implementation task and before closeout. Review must
check:

- no production behavior drift,
- no public API or fixture schema drift,
- no renderer or adapter dependency drift,
- helpers are genuinely shared and local to runtime tests,
- test names and filters remain practical for targeted nextest runs.

## Verification Gate

Run `verify-rust-workstream` before marking JRTSS-020, JRTSS-030, or the full lane complete. Fresh
command evidence must be recorded here before completion claims.

## Notes

This lane is allowed to reduce duplication and improve test locality, but it must not turn into a
feature lane. New gesture behavior, public fixture formats, core-test organization, and adapter
renderer smoke tests are separate workstreams.
