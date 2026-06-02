# Quality Guidelines

Runtime quality means deterministic store behavior, explicit adapter contracts,
and renderer-free conformance coverage.

## Required Patterns

- Keep `#![deny(unsafe_code)]`.
- Keep manifest tests that reject Fret, UI, renderer, platform, `wgpu`, and
  `winit` dependencies.
- Route graph mutations through `jellyflow-core::ops` and store dispatch
  pipeline helpers.
- Use `runtime::policy` for effective node/port/edge interaction policy.
- Add conformance coverage for adapter-facing behavior before relying on
  renderer smoke tests.
- Add public-surface coverage for new public runtime/conformance/XyFlow APIs.

## Forbidden Patterns

- Adding renderer/platform/Fret dependencies to prove behavior.
- Adding adapter-facing behavior without a headless conformance or focused
  runtime test.
- Duplicating XyFlow projection/callback logic outside `runtime::xyflow`.
- Expanding a focused capability into unrelated follow-ons inside the same task.
- Treating closed workstream follow-ons as already approved implementation scope.

## Validation Gates

Choose the smallest meaningful gate first:

```text
cargo fmt --check
cargo nextest run -p jellyflow-runtime <filter>
cargo nextest run -p jellyflow-runtime --test public_surface
cargo nextest run -p jellyflow-runtime conformance
cargo nextest run -p jellyflow-runtime adapter_conformance
cargo test --manifest-path templates/headless-adapter/Cargo.toml
cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check
cargo clippy -p jellyflow-runtime --all-targets -- -D warnings
python3 tools/check_no_fret_dependencies.py
python3 tools/check_external_consumer_smoke.py
git diff --check
```

Use workspace-wide gates when manifests, public API, shared store behavior, or
core/runtime contracts change together.

## Review Checklist

- Does the change preserve the headless boundary from ADR 0001 and ADR 0003?
- Does it use `runtime::policy` where effective interaction policy matters?
- Does adapter-facing behavior have conformance or template coverage?
- Are XyFlow names isolated to `runtime::xyflow` or explicit conformance APIs?
- Are public re-exports and `NodeGraphStore` methods intentional?
