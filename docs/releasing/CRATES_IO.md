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
cargo fmt --check
cargo check --workspace --locked
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 tools/check_no_fret_dependencies.py
python3 tools/check_external_consumer_smoke.py
git diff --check
```

The CI workflow runs the same common gates on Ubuntu. It intentionally does not
publish crates.

## Package Contents

Review the files Cargo would package:

```text
cargo package --list -p jellyflow-core
cargo package --list -p jellyflow-layout
cargo package --list -p jellyflow-runtime
cargo package --list -p jellyflow
```

The package list should not include Trellis task files, `repo-ref`, historical
workstream archives, or renderer/platform assets. Each crate package should
include its crate README and the files needed to build that crate from crates.io.

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

Actual publishing, release tags, version bumps, and a publishing workflow are
out of scope for the release-readiness audit task.
