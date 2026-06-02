# Quality Guidelines

Quality in `jellyflow-core` means stable headless data contracts, deterministic
transactions, and clear invariants.

## Required Patterns

- Keep `#![deny(unsafe_code)]`.
- Keep manifest tests that reject Fret, UI, renderer, platform, `wgpu`, and
  `winit` dependencies.
- Normalize and validate graph transactions through existing `ops` helpers.
- Add tests near the module family being changed.
- Use typed IDs and structured model fields instead of raw strings for graph
  references.

## Forbidden Patterns

- Adding renderer/platform/Fret dependencies.
- Moving persisted policy/layout fields out of `Graph` without ADR-backed schema
  migration planning.
- Duplicating mutation logic in runtime or tests when `ops` helpers already
  model the operation.
- Expanding crate-root public API as incidental fallout from a private change.

## Validation Gates

Choose the smallest meaningful gate first:

```text
cargo fmt --check
cargo nextest run -p jellyflow-core <filter>
cargo nextest run -p jellyflow-core
cargo clippy -p jellyflow-core --all-targets -- -D warnings
python3 tools/check_no_fret_dependencies.py
git diff --check
```

Use workspace-wide gates when a change touches public API, manifests, shared
transaction behavior, or cross-crate callers.

## Review Checklist

- Does the change preserve the headless boundary from ADR 0001?
- Does it respect the v1 model-policy decision from ADR 0002?
- Are public re-exports intentional?
- Are invariant failures tested through structured errors or reports?
- Did the implementation reuse existing `ops`/validation helpers?
