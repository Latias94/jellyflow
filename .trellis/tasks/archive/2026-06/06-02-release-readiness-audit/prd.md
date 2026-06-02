# Release readiness audit

## Goal

Make Jellyflow's pre-release state explicit and actionable before any crates.io
publish attempt. The task should audit and, where low-risk gaps are found,
bring the repository to a minimal release-readiness baseline: publish metadata,
CI gates, package contents, dry-run evidence, and release documentation.

The user value is confidence that Jellyflow can move from "history-preserved
extraction" to "publishable Rust workspace" without mixing renderer behavior
work into packaging work.

## Confirmed Facts

- Jellyflow is a two-crate Cargo workspace:
  - `jellyflow-core`
  - `jellyflow-runtime`, which depends on `jellyflow-core`
- Current package metadata already includes author, license, repository,
  homepage, documentation, description, keywords, categories, and crate-local
  README paths.
- Workspace package metadata currently uses:
  - `authors = ["Mingzhen Zhuang <superfrankie621@gmail.com>"]`
  - `license = "MIT OR Apache-2.0"`
  - `repository = "https://github.com/Latias94/jellyflow"`
  - `homepage = "https://github.com/Latias94/jellyflow"`
  - `rust-version = "1.92"`
- Local toolchain evidence: `rustc 1.96.0` and `cargo 1.96.0`.
- The repository has `Cargo.lock`, `LICENSE-MIT`, and `LICENSE-APACHE`.
- The repository currently has no `.github/workflows` directory.
- `README.md` explicitly says crates.io publishing is blocked until package
  metadata, CI, package lists, and publish dry-runs are verified.
- Existing validation gates already documented by Jellyflow:
  - `cargo fmt --check`
  - `cargo check --workspace`
  - `cargo nextest run --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `python3 tools/check_no_fret_dependencies.py`
  - `python3 tools/check_external_consumer_smoke.py`
  - `git diff --check`
- Adapter-facing validation gates already documented by Jellyflow:
  - `cargo run -p jellyflow-runtime --example conformance_harness -- check <fixture-dir>`
  - `cargo run -p jellyflow-runtime --example conformance_harness -- approve <fixture-dir>`
  - `cargo test --manifest-path templates/headless-adapter/Cargo.toml`
  - `cargo run --manifest-path templates/headless-adapter/Cargo.toml -- check`
- `repo-ref/merman` is the author's reference Rust library for metadata,
  release docs, and CI style. It uses the same author identity and
  MIT/Apache-2.0 licensing pattern.
- `repo-ref/merman` has:
  - a `ci.yml` workflow using checkout, exact Rust toolchain install,
    `cargo-nextest`, and `Swatinem/rust-cache`;
  - a `release-crates.yml` workflow that publishes crates in dependency order
    with retry;
  - docs explaining that crates with unpublished workspace dependencies may
    fail `cargo publish --dry-run` until dependency crates exist on crates.io.
- `repo-ref/xyflow` is the behavior source the user plans to replicate. It is a
  behavior/reference source for Jellyflow semantics, not a source for Jellyflow
  Rust release metadata.

## Requirements

- Audit Jellyflow package metadata against crates.io/docs.rs expectations and
  the `repo-ref/merman` style.
- Decide whether `rust-version = "1.92"` is intentional release policy or a
  value that needs correction. Do not lower MSRV without compile evidence.
- Add a minimal CI workflow if missing. The workflow should cover Jellyflow's
  common Rust gates and smoke scripts without depending on `repo-ref`.
- Audit package contents with `cargo package --list` for both publishable crates.
- Run publish dry-runs where crates.io dependency constraints allow it.
- Document the first-release dependency-order caveat:
  `jellyflow-runtime` depends on `jellyflow-core`, so runtime publish dry-runs
  may fail until `jellyflow-core` exists on crates.io.
- Update release/readiness docs so future agents can distinguish:
  - CI readiness
  - package contents readiness
  - dry-run readiness
  - actual crates.io publishing, which stays out of scope for this task
- Keep this task packaging-focused. Do not refactor runtime behavior, adapter
  conformance, schema migration, or XyFlow parity logic.

## Acceptance Criteria

- [x] Package metadata for both Jellyflow crates is reviewed and either updated
      or explicitly accepted as-is.
- [x] MSRV policy is reviewed; `rust-version = "1.92"` is either validated or
      left with a documented reason and follow-up.
- [x] A minimal GitHub Actions CI workflow exists or the task documents why CI
      remains intentionally absent.
- [x] CI uses the documented Jellyflow common gates and does not require
      renderer, Fret, or `repo-ref` dependencies.
- [x] `cargo package --list -p jellyflow-core` output is reviewed.
- [x] `cargo package --list -p jellyflow-runtime` output is reviewed.
- [x] `cargo publish --dry-run -p jellyflow-core` is run or the blocker is
      recorded.
- [x] `cargo publish --dry-run -p jellyflow-runtime` is run or the expected
      unpublished-dependency blocker is recorded.
- [x] Release/readiness documentation records publish order:
      `jellyflow-core` before `jellyflow-runtime`.
- [x] `README.md` and/or release docs no longer contain a vague publish blocker;
      remaining blockers are concrete and evidence-backed.
- [x] No actual crates.io publish, release tag, or package version bump happens
      in this task.

## Out Of Scope

- Publishing crates to crates.io.
- Creating release tags.
- Version bumping.
- Adding renderer smoke tests.
- Implementing new XyFlow parity behavior from `repo-ref/xyflow`.
- Schema migration or persisted model reshaping.
- Reintroducing the old workstream system.

## Resolved Scope Decision

This task adds CI plus release-readiness documentation only. It does not add an
actual `release-crates.yml` publishing workflow. Publishing automation is
deferred until package dry-runs and first-release dependency-order blockers are
fully understood.
