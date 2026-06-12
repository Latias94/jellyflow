# Publishing To crates.io

Jellyflow is a Cargo workspace with four publishable crates:

1. `jellyflow-core`
2. `jellyflow-layout`
3. `jellyflow-runtime`
4. `jellyflow`

`jellyflow-layout` depends on `jellyflow-core`, `jellyflow-runtime` depends on
`jellyflow-core` and `jellyflow-layout`, and the top-level `jellyflow` facade
depends on all three lower-level crates. Publish and dry-run checks must respect
that order.

## Release Gates

Run these before any publish attempt:

```text
cargo fmt --all --check
cargo check --workspace --locked
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo bench -p jellyflow-runtime --bench rendering_query -- --test
cargo bench -p jellyflow-runtime --bench schema_create_node -- --test
python3 tools/check_no_fret_dependencies.py
python3 tools/check_external_consumer_smoke.py
git diff --check
```

The CI workflow runs common gates on pull requests and pushes. The manual
release preflight workflow additionally verifies versions and writes package
file lists as artifacts for release review. The crates publishing workflow
publishes in dependency order from a `v*` tag or manual dispatch. The GitHub
Release workflow creates or updates release notes for the same tag.

## Package Contents

Review the files Cargo would package:

```text
cargo package --locked --no-verify --list -p jellyflow-core
cargo package --locked --no-verify --list -p jellyflow-layout
cargo package --locked --no-verify --list -p jellyflow-runtime
cargo package --locked --no-verify --list -p jellyflow
```

The package list should not include Trellis task files, `repo-ref`, historical
workstream archives, or renderer/platform assets. Each crate package should
include its crate README and the files needed to build that crate from crates.io.
`--no-verify` is used for package-list review so first-release dependent crates
do not fail only because earlier Jellyflow crates are not visible on crates.io
yet.

## Dry Runs

Run dry-runs in dependency order:

```text
cargo publish --dry-run -p jellyflow-core
cargo publish --dry-run -p jellyflow-layout
cargo publish --dry-run -p jellyflow-runtime
cargo publish --dry-run -p jellyflow
```

Cargo rewrites workspace path dependencies into registry dependencies during
packaging and publishing. For a first release, dry-runs for crates that depend
on unpublished workspace crates may fail because those dependency names are not
available on crates.io yet. Treat that as an expected dependency-order blocker
only after confirming no unrelated metadata or packaging error appears earlier
in the output.

## Publish Order

Publish leaf crates first:

```text
cargo publish -p jellyflow-core
cargo publish -p jellyflow-layout
cargo publish -p jellyflow-runtime
cargo publish -p jellyflow
```

Actual crates.io publishing is handled by
`.github/workflows/release-crates.yml`. It waits for each published crate
version to become visible before publishing the next dependent crate. GitHub
Release notes are handled separately by `.github/workflows/release.yml`.
