# crates.io Release Guide

Jellyflow publishes the crates listed in `scripts/release-crates.txt` in dependency order. Release
automation deliberately stays small: CI proves the source, the release workflow validates the tag
and version, Cargo publishes each crate, and the workflow waits for registry visibility before
continuing.

## Local release gates

Run from the repository root:

```text
cargo fmt --all --check
cargo nextest run --workspace --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 tools/check_no_fret_dependencies.py
python3 -m unittest discover -s tools/tests -p "test_*.py"
python3 tools/check_external_consumer_smoke.py
```

The dependency check delegates graph resolution to `cargo tree`; it only enforces Jellyflow's
headless, adapter-separation, and no-Fret policies. It does not implement a second Cargo resolver.

The external consumer smoke packages the release crates, extracts the locally produced `.crate`
archives, patches three small consumers to those sources, and runs `cargo check`. It verifies the
actual package boundary on default-branch push CI without maintaining a provenance engine or
duplicating adapter behavior tests. Pull requests rely on the normal workspace gates; the release
workflow later requires a successful push CI run for the exact source SHA.

## Version preparation

Before tagging a release:

1. Move the release notes from `Unreleased` into a non-empty `## [<version>]` section.
2. Update the workspace version, internal dependency constraints, and the nested Open GPUI example
   version.
3. Refresh both lockfiles.
4. Run:

```text
python3 scripts/release-version.py check --version <version>
cargo metadata --locked --format-version 1
cargo metadata --manifest-path examples/canvas-jellyflow/Cargo.toml \
  --locked --format-version 1
```

Review package contents when release metadata changes:

```text
cargo package --locked --list -p jellyflow-core
cargo package --locked --list -p jellyflow-layout
cargo package --locked --list -p jellyflow-runtime
cargo package --locked --list -p jellyflow-egui
cargo package --locked --list -p jellyflow-open-gpui
```

## Trust boundary

The release workflow is triggered only by a `release-crates` repository dispatch carrying an
existing `v<version>` tag. It resolves that tag to a full source commit, requires the commit to be
an ancestor of the trusted default-branch workflow commit, verifies the default branch is
protected, and requires a successful CI push run for the exact source SHA.

GitHub Actions remain pinned to full commit SHAs. This is a supply-chain control, not an archive
digest protocol. Configure the `crates.io` environment with required reviewers, prevent
self-review, keep `CARGO_REGISTRY_TOKEN` as an environment secret, and protect `refs/tags/v*`
against update and deletion.

Trigger a reviewed tag with:

```text
gh api repos/OWNER/REPOSITORY/dispatches \
  -f event_type=release-crates \
  -F 'client_payload[release_tag]=v0.4.0'
```

## Publication behavior

The workflow:

1. checks out the immutable source SHA;
2. validates the version and changelog;
3. publishes each crate from `scripts/release-crates.txt` with `cargo publish --locked`;
4. waits for that version to become visible before publishing its dependents;
5. verifies the protected tag still resolves to the guarded source SHA;
6. creates one GitHub Release from the matching changelog section.

The workflow does not rebuild candidate archives, download crates.io archives, compare SHA-256
digests, classify retryable GraphQL errors, or reconcile partial releases automatically. If a run
publishes only part of the crate set, stop and assess the registry state explicitly; do not hide the
incident behind a growing recovery engine. Usually the safe resolution is to fix the cause and
publish a new version.
