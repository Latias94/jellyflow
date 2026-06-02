# Release Readiness Audit Design

## Boundary

This task is a packaging and repository-readiness slice. It may touch release
metadata, GitHub Actions CI, release documentation, and status docs. It must not
change Jellyflow runtime behavior, public graph semantics, adapter conformance
logic, or schema boundaries.

Read-only references:

- `repo-ref/merman`: author metadata, release docs, CI workflow style, crates.io
  dependency-order practices.
- `repo-ref/xyflow`: behavioral source for future parity work. It should not
  drive release metadata or CI shape in this task.

## Architecture

Jellyflow has a simple publish dependency graph:

```text
jellyflow-core
    ^
    |
jellyflow-runtime
```

Publishing and dry-run checks must respect that order. Cargo rewrites workspace
path dependencies into registry dependencies during packaging and publishing.
For the first release, `jellyflow-runtime` may fail a crates.io dry-run until
`jellyflow-core` is already available on crates.io. That failure is acceptable
only if it is documented precisely and no unrelated packaging error is hidden by
it.

## CI Shape

Use `repo-ref/merman/.github/workflows/ci.yml` as a style reference, but keep the
Jellyflow workflow smaller:

- checkout
- exact Rust toolchain install matching Jellyflow's MSRV policy
- cargo-nextest install
- rust-cache
- `cargo fmt --check`
- `cargo check --workspace --locked`
- `cargo nextest run --workspace --cargo-quiet`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `python3 tools/check_no_fret_dependencies.py`
- `python3 tools/check_external_consumer_smoke.py`

The first CI version can run on Ubuntu only if the task prioritizes minimal
setup. A three-OS matrix is stronger for a reusable headless Rust library, but
it costs more CI time and is not strictly required to unblock local package
readiness.

## Metadata Design

Keep the author's identity aligned with `repo-ref/merman` unless the user asks
for a different public identity:

```toml
authors = ["Mingzhen Zhuang <superfrankie621@gmail.com>"]
license = "MIT OR Apache-2.0"
repository = "https://github.com/Latias94/jellyflow"
homepage = "https://github.com/Latias94/jellyflow"
```

For package metadata, review rather than blindly change:

- `description`
- `keywords`
- `categories`
- `documentation`
- `readme`
- `rust-version`
- optional `package.metadata.docs.rs`

Any MSRV reduction must be backed by a successful compile/test check on that
toolchain. If exact MSRV validation is not available locally, keep `1.92` and
document it as the current release policy.

## Release Documentation

Add a small release-readiness document only if it removes ambiguity from
`README.md`. The document should cover:

- release gates
- publish order
- package list checks
- dry-run caveats for first release
- what remains out of scope

The README can keep a short repository-status paragraph and link to the detailed
doc.

## Rollback

Rollback is straightforward because the task is repository metadata only:

- remove the new CI workflow if it proves too broad;
- revert release-doc changes if package dry-runs expose a different first
  release strategy;
- do not touch runtime code, so no behavioral rollback should be needed.
