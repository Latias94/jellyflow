# Release Readiness

## Scenario: CI And crates.io Readiness

### 1. Scope / Trigger

Use this spec when a task changes any of:

- `.github/workflows/`
- workspace or crate package metadata in `Cargo.toml`
- `docs/releasing/`
- README repository-status publishing text
- release or publishing automation

This is repository infrastructure. Keep it separate from runtime behavior,
adapter conformance, XyFlow parity, schema migration, and renderer smoke tests.

### 2. Signatures

Required local commands for release-readiness evidence:

```text
cargo fmt --check
cargo check --workspace --locked
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 tools/check_no_fret_dependencies.py
python3 tools/check_external_consumer_smoke.py
cargo package --list -p jellyflow-core
cargo package --list -p jellyflow-runtime
cargo publish --dry-run -p jellyflow-core
cargo publish --dry-run -p jellyflow-runtime
git diff --check
```

When validating the declared MSRV, run the Rust gates with the exact toolchain:

```text
cargo +<rust-version> check --workspace --locked
cargo +<rust-version> nextest run --workspace
cargo +<rust-version> clippy --workspace --all-targets -- -D warnings
```

### 3. Contracts

- Publish order is `jellyflow-core` before `jellyflow-runtime`.
- `jellyflow-runtime` dry-runs may fail before the first release if
  `jellyflow-core` is not available on crates.io. Record this as an expected
  dependency-order blocker only if no earlier metadata or packaging error is
  present.
- CI must run common Jellyflow gates without depending on `repo-ref`.
- CI and release docs must not introduce renderer, Fret, UI, platform, `wgpu`,
  `winit`, or egui dependencies into headless crates.
- Do not add `release-crates.yml`, release tags, version bumps, or actual
  publishing unless the active task explicitly includes publishing automation.

Package metadata should remain explicit for each publishable crate:

- `authors`
- `license`
- `repository`
- `homepage`
- `documentation`
- `readme`
- `description`
- `keywords`
- `categories`
- `rust-version`

### 4. Validation & Error Matrix

| Condition | Required response |
| --- | --- |
| Package list includes `.trellis`, `repo-ref`, historical workstream archives, or renderer/platform assets | Treat as packaging blocker; fix include/exclude or crate layout before dry-run claims. |
| `jellyflow-core` dry-run fails | Treat as release blocker; record exact error and do not claim crates are ready. |
| `jellyflow-runtime` dry-run fails because `jellyflow-core` is missing from crates.io | Expected first-release order blocker; document it and keep publish order explicit. |
| `jellyflow-runtime` dry-run fails for any other reason | Treat as release blocker. |
| MSRV toolchain fails `check`, tests, or clippy | Do not lower or claim MSRV; either fix code or document follow-up. |
| CI requires `repo-ref` or renderer dependencies | Reject the CI shape; release-readiness CI must validate the standalone headless repo. |

### 5. Good/Base/Bad Cases

Good:

- CI runs fmt, check, nextest, clippy, no-Fret smoke, and external consumer
  smoke on a fixed Rust toolchain.
- Release docs record package-list commands, dry-run commands, publish order,
  and the first-release dependency-order caveat.
- `jellyflow-core` dry-run passes; `jellyflow-runtime` dry-run is either passes
  or records only the expected missing-core crates.io blocker.

Base:

- Ubuntu-only CI is acceptable for minimal release readiness when the task does
  not explicitly ask for a cross-platform matrix.

Bad:

- Adding a publish workflow before package dry-runs are understood.
- Claiming runtime is publish-ready while hiding a non-dependency-order dry-run
  failure.
- Copying renderer, UI, or parity smoke gates from `repo-ref/xyflow` into
  Jellyflow headless crate CI.

### 6. Tests Required

For release-readiness changes, run or document blockers for:

- common Rust gates;
- both dependency smoke scripts;
- package-list checks for both crates;
- dry-run checks in dependency order;
- `git diff --check`;
- YAML syntax or parse check when changing GitHub Actions workflow files.

### 7. Wrong vs Correct

#### Wrong

```text
cargo publish --dry-run -p jellyflow-runtime
# fails because jellyflow-core is missing from crates.io
# conclusion: runtime metadata is broken
```

#### Correct

```text
cargo publish --dry-run -p jellyflow-core
# passes

cargo publish --dry-run -p jellyflow-runtime
# fails only because jellyflow-core is not on crates.io yet
# conclusion: expected first-release dependency-order blocker
```

#### Wrong

```yaml
run: cargo run -p future-renderer-smoke
```

#### Correct

```yaml
run: python3 tools/check_external_consumer_smoke.py
```

Headless repository CI validates public Jellyflow APIs and dependency boundaries.
Renderer smoke tests belong in future adapter crates.
