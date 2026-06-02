# Publishing To crates.io

Jellyflow is a Cargo workspace with two publishable crates:

1. `jellyflow-core`
2. `jellyflow-runtime`

`jellyflow-runtime` depends on `jellyflow-core`, so publish and dry-run checks
must respect that order.

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
cargo package --list -p jellyflow-runtime
```

The package list should not include Trellis task files, `repo-ref`, historical
workstream archives, or renderer/platform assets. Each crate package should
include its crate README and the files needed to build that crate from crates.io.

## Dry Runs

Run dry-runs in dependency order:

```text
cargo publish --dry-run -p jellyflow-core
cargo publish --dry-run -p jellyflow-runtime
```

Cargo rewrites workspace path dependencies into registry dependencies during
packaging and publishing. For a first release, the `jellyflow-runtime` dry-run
may fail because `jellyflow-core` is not available on crates.io yet. Treat that
as an expected dependency-order blocker only after confirming no unrelated
metadata or packaging error appears earlier in the output.

## Publish Order

Publish leaf crates first:

```text
cargo publish -p jellyflow-core
cargo publish -p jellyflow-runtime
```

Actual publishing, release tags, version bumps, and a publishing workflow are
out of scope for the release-readiness audit task.
