# Jellyflow Runtime Test Surface Split v1 - Handoff

Status: Closed
Last updated: 2026-06-01

## Current State

The workstream is closed. JRTSS-010, JRTSS-020, JRTSS-030, and JRTSS-040 are complete.

JRTSS-020 split the runtime conformance, adapter-conformance, and harness test surface into focused
test-only submodules. Production runtime behavior, public API paths, fixture JSON schema,
conformance traces, callback payloads, and renderer boundaries remain unchanged.

JRTSS-030 split runtime drag and selection tests into focused scenario/support submodules.
`viewport.rs` and `auto_pan.rs` remain compact direct test modules and were validated without
splitting.

## Active Task

None.

## Decisions Since Opening

- Treat this lane as a test-only refactor.
- Preserve production runtime behavior, public API paths, fixture JSON schema, conformance traces,
  and callback payloads.
- Keep `wgpu`, egui, Fret, screenshots, pixel checks, browser dependencies, and renderer smoke tests
  outside `jellyflow-core` and `jellyflow-runtime`.
- Keep `jellyflow-core` test organization out of scope unless a separate follow-on is opened.
- Keep JRTSS-030 focused on drag and interaction-heavy runtime tests. Do not add gesture behavior.

## Blockers

- None known.

## Validation So Far

- `cargo fmt --check`: pass.
- `cargo nextest run -p jellyflow-runtime conformance`: pass, 26 tests.
- `cargo nextest run -p jellyflow-runtime --example conformance_harness`: pass, 3 tests.
- `cargo nextest run -p jellyflow-runtime --test public_surface`: pass, 3 tests.
- `cargo nextest run -p jellyflow-runtime drag`: pass, 10 tests.
- `cargo nextest run -p jellyflow-runtime selection`: pass, 9 tests.
- `cargo nextest run -p jellyflow-runtime viewport`: pass, 16 tests.
- `cargo nextest run -p jellyflow-runtime`: pass, 177 tests.
- `cargo clippy -p jellyflow-runtime --all-targets -- -D warnings`: pass.
- `jq empty docs/workstreams/jellyflow-runtime-test-surface-split-v1/WORKSTREAM.json docs/workstreams/jellyflow-runtime-test-surface-split-v1/TASKS.jsonl docs/workstreams/jellyflow-runtime-test-surface-split-v1/CAMPAIGNS.jsonl docs/workstreams/jellyflow-runtime-test-surface-split-v1/CONTEXT.jsonl`: pass.
- `git diff --check`: pass.

## Next Recommended Action

None in this workstream. Commit the closed lane when the user approves.

## Follow-On Candidates

- `jellyflow-core` test-surface organization if core test navigation becomes the next bottleneck.
- New gesture-kernel workstreams for resize, reconnect lifecycle, or richer pan/zoom behavior.
- Adapter-crate renderer smoke tests for future wgpu, egui, or Fret integrations.
