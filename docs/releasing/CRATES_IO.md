# Publishing To crates.io

Jellyflow is a Cargo workspace with five publishable crates:

1. `jellyflow-core`
2. `jellyflow-layout`
3. `jellyflow-runtime`
4. `jellyflow-egui`
5. `jellyflow-open-gpui`

`jellyflow-layout` and `jellyflow-runtime` each depend directly on `jellyflow-core`. Both UI
adapters depend directly on `jellyflow-core` and `jellyflow-runtime`; neither adapter depends on
the other or on the optional layout package. Publish and dry-run checks use the order above.

## Release Gates

Run these before any publish attempt:

```sh
set -euo pipefail
cargo fmt --all -- --check
cargo check --workspace --locked
cargo nextest run --workspace --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo bench -p jellyflow-layout --features benchmark-internals --bench layout_engines --locked -- --test
python3 tools/check_no_fret_dependencies.py
python3 -m unittest discover -s tools/tests -p "test_*.py"
python3 tools/check_external_consumer_smoke.py
cargo check --manifest-path examples/canvas-jellyflow/Cargo.toml --workspace --locked
cargo fmt --manifest-path examples/canvas-jellyflow/Cargo.toml --all -- --check
cargo nextest run --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow --locked --no-fail-fast
cargo test --manifest-path examples/canvas-jellyflow/Cargo.toml --workspace --doc --locked
cargo clippy --manifest-path examples/canvas-jellyflow/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo build --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow --bins --locked --features real-window-smoke
capture_output="$(mktemp)"
trap 'rm -f "$capture_output"' EXIT
python3 tools/run_real_window_smoke_matrix.py \
  --binary examples/canvas-jellyflow/target/debug/open-gpui-canvas-jellyflow \
  --receipt-validator examples/canvas-jellyflow/target/debug/smoke_receipt_contract \
  --capture-root target/real-window-smoke \
  --timeout-seconds 60 \
  --capture-dir-output "$capture_output"
python3 tools/check_real_window_smoke_evidence.py \
  --capture-dir-from-output "$capture_output" \
  --receipt-validator examples/canvas-jellyflow/target/debug/smoke_receipt_contract
git diff --check
```

The CI workflow runs common gates on pull requests and pushes. The reusable
release preflight workflow additionally verifies versions, runs eight Open GPUI interaction
processes plus one semantic-LOD process, and writes package lists plus 75 state screenshots and a
sorted screenshot manifest as artifacts for release review. The packaged-consumer smoke assembles
all five `.crate` archives once, serves them from a temporary sparse registry, then compiles three
locked consumers for the headless, egui, and Open GPUI boundaries. Each consumer
declares only the package roots its source uses. Exact `cargo tree` assertions
reject layout or cross-framework leakage, while the three closure sets together
must cover all five candidate archives at their expected versions and checksums.
The production Open GPUI dependency disables default features and does not enable
`test-support`; the additional source smoke owns framework test-support interaction
coverage.

The crates publishing workflow exists only on the default branch and accepts a
fixed `release-crates` repository-dispatch event. Its `release_tag` payload is
untrusted input: the workflow fetches that existing tag independently, resolves
its exact object and commit IDs, requires the tag commit to be an ancestor of
the default-branch commit that received the dispatch, and requires a successful
CI push run for the exact release SHA. The same SHA must pass the reusable
release preflight workflow before the `crates.io` environment can publish.

Trigger a release only after pushing the reviewed tag:

```text
gh api repos/OWNER/REPOSITORY/dispatches \
  -f event_type=release-crates \
  -F 'client_payload[release_tag]=v0.1.0'
```

Configure the `crates.io` environment with required reviewers, prevent
self-review, keep `CARGO_REGISTRY_TOKEN` as an environment secret rather than a
repository-wide secret, and restrict deployments to the protected default
branch used by repository dispatch. Separately protect `refs/tags/v*` creation,
update, and deletion.
These repository settings are required because workflow YAML cannot enforce
environment approval, dispatch authorization, or tag policy by itself. The
GitHub Release job creates or updates release notes for the same tag. It
revalidates the tag immediately before and after the GitHub Release mutation; a
post-operation mismatch fails the job. This detects movement but cannot make the
remote tag and release update atomic, so release tags must still be protected
against movement or deletion.

## Package Contents

Review the files Cargo would package:

```text
cargo package --locked --list -p jellyflow-core
cargo package --locked --list -p jellyflow-layout
cargo package --locked --list -p jellyflow-runtime
cargo package --locked --list -p jellyflow-egui
cargo package --locked --list -p jellyflow-open-gpui
```

The package list should not include `repo-ref`, historical workstream archives,
or renderer/platform assets. Each crate package should include its crate README
and the files needed to build that crate from crates.io.

Do not use a successful `cargo package --no-verify` invocation as build
evidence. The external consumer smoke uses `--no-verify` only to assemble the
candidate archives because Cargo's built-in verification resolves same-version
workspace dependencies from crates.io. It then verifies the actual archives by
checksum, resolves them through the temporary registry, generates a consumer
lockfile, and runs that consumer with `--locked`. A missing file, invalid
normalized manifest, stale public API, or cross-crate mismatch fails that later
compile.

## Dry Runs

Run dry-runs in dependency order:

```text
cargo publish --locked --dry-run -p jellyflow-core
cargo publish --locked --dry-run -p jellyflow-layout
cargo publish --locked --dry-run -p jellyflow-runtime
cargo publish --locked --dry-run -p jellyflow-egui
cargo publish --locked --dry-run -p jellyflow-open-gpui
```

Cargo rewrites workspace path dependencies into registry dependencies during
packaging and publishing. For a first release or same-version multi-crate
release, dry-runs for crates that depend on unpublished workspace crates may
fail because those dependency versions are not available on crates.io yet. Treat
that as an expected dependency-order blocker only after confirming no unrelated
metadata or packaging error appears earlier in the output. The isolated
packaged consumers remain the local proof that the complete candidate
archive set compiles without hidden cross-adapter dependencies before those
versions are visible on crates.io.

## Publish Order

Publish leaf crates first:

```text
cargo publish --locked -p jellyflow-core
cargo publish --locked -p jellyflow-layout
cargo publish --locked -p jellyflow-runtime
cargo publish --locked -p jellyflow-egui
cargo publish --locked -p jellyflow-open-gpui
```

Actual crates.io publishing and GitHub Release creation are handled by
`.github/workflows/release-crates.yml`. It waits for each published crate
version to become visible before publishing the next dependent crate, then
creates or updates the GitHub Release from the same guarded tag and commit.

Before creating the tag, promote the release notes from `Unreleased` into one non-empty exact
`## [<version>]` changelog section. Then update `workspace.package.version`, every internal
workspace dependency constraint, and the nested Open GPUI product package
version. Refresh the root and `examples/canvas-jellyflow` lockfiles, then run. The version check
rejects a missing or empty exact changelog section:

```text
python3 scripts/release-version.py check --version <version>
cargo metadata --locked --format-version 1
cargo metadata --manifest-path examples/canvas-jellyflow/Cargo.toml --locked --format-version 1
```

A candidate archive smoke can validate an uncommitted working tree and can
intentionally shadow an already-published version in its temporary registry;
crates.io cannot. The release workflow therefore still requires workspace and
product versions to match the tag and skips only versions already present on
crates.io with a byte-identical archive. A same-version archive with different
contents fails the release rather than mixing commits.
