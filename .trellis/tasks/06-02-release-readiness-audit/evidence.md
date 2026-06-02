# Release Readiness Audit Evidence

Date: 2026-06-02

## Metadata

Reviewed with:

```text
cargo +1.92.0 metadata --no-deps --format-version 1
```

Accepted as-is:

| Crate | Version | Rust version | License | Repository | Documentation | Readme |
| --- | --- | --- | --- | --- | --- | --- |
| `jellyflow-core` | `0.1.0` | `1.92` | `MIT OR Apache-2.0` | `https://github.com/Latias94/jellyflow` | `https://docs.rs/jellyflow-core` | `README.md` |
| `jellyflow-runtime` | `0.1.0` | `1.92` | `MIT OR Apache-2.0` | `https://github.com/Latias94/jellyflow` | `https://docs.rs/jellyflow-runtime` | `README.md` |

Author and license metadata match the `repo-ref/merman` style.

## MSRV

Local toolchain evidence:

```text
rustc 1.96.0 (ac68faa20 2026-05-25)
cargo 1.96.0 (30a34c682 2026-05-25)
```

MSRV validation used installed Rust `1.92.0`:

```text
cargo +1.92.0 fmt --check
cargo +1.92.0 check --workspace --locked
cargo +1.92.0 nextest run --workspace
cargo +1.92.0 clippy --workspace --all-targets -- -D warnings
```

Result: all passed. `cargo +1.92.0 nextest run --workspace` ran 377 tests, all
passed.

## CI And Docs

Added:

- `.github/workflows/ci.yml`
- `docs/releasing/CRATES_IO.md`
- `.trellis/spec/repository/backend/release-readiness.md`

Updated:

- `README.md`
- `.trellis/config.yaml`
- `.trellis/spec/guides/index.md`

The CI workflow runs common Jellyflow gates on Ubuntu and does not publish
crates. The release-readiness docs record publish order and first-release
dry-run caveats.

The repository-level Trellis spec now records CI/package/dry-run contracts for
future release-readiness work. `python3 ./.trellis/scripts/get_context.py --mode
packages` discovers the new `repository` package and its backend spec.

YAML parse check:

```text
ruby -e "require 'yaml'; YAML.load_file('.github/workflows/ci.yml'); puts 'ci yaml parses'"
```

Result: passed.

## Package Contents

Commands:

```text
cargo +1.92.0 package --list -p jellyflow-core
cargo +1.92.0 package --list -p jellyflow-runtime
```

Reviewed package lists:

- `jellyflow-core`: 109 package-list entries.
- `jellyflow-runtime`: 367 package-list entries.

No Trellis task files, `repo-ref`, historical workstream archives, or renderer
assets appeared in either crate package list.

## Publish Dry-Runs

Command:

```text
cargo +1.92.0 publish --dry-run -p jellyflow-core
```

Result: passed. Cargo packaged 109 files, 315.2KiB uncompressed and 57.2KiB
compressed, verified the crate, then aborted upload due to dry-run.

Command:

```text
cargo +1.92.0 publish --dry-run -p jellyflow-runtime
```

Result: failed with the expected first-release dependency-order blocker:

```text
no matching package named `jellyflow-core` found
location searched: crates.io index
required by package `jellyflow-runtime v0.1.0`
```

This matches the documented Cargo behavior for workspace path dependencies that
are rewritten to registry dependencies during packaging/publishing.

## Boundary Gates

Commands:

```text
python3 tools/check_no_fret_dependencies.py
python3 tools/check_external_consumer_smoke.py
git diff --check
```

Results:

- No `fret` or `fret-*` packages were found in `jellyflow-core` or
  `jellyflow-runtime` dependency trees within depth 2.
- External consumer smoke passed.
- Headless adapter template smoke produced a suite report.
- `git diff --check` passed.

## Out Of Scope Confirmed

This task did not:

- publish to crates.io;
- create a release tag;
- bump package versions;
- add `release-crates.yml`;
- change runtime/core behavior.
