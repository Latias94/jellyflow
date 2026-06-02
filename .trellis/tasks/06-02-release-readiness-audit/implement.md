# Release Readiness Audit Implementation Plan

## Checklist

- [x] Re-read relevant Trellis specs before editing repository files.
- [x] Review current Jellyflow package metadata and compare with
      `repo-ref/merman` metadata style.
- [x] Decide whether metadata needs edits or can be accepted as-is.
- [x] Create minimal `.github/workflows/ci.yml` if CI remains in scope.
- [x] Add release-readiness documentation if README's current blocker needs a
      durable target.
- [x] Run `cargo fmt --check`.
- [x] Run `cargo check --workspace --locked`.
- [x] Run `cargo nextest run --workspace`.
- [x] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Run `python3 tools/check_no_fret_dependencies.py`.
- [x] Run `python3 tools/check_external_consumer_smoke.py`.
- [x] Run `cargo package --list -p jellyflow-core`.
- [x] Run `cargo package --list -p jellyflow-runtime`.
- [x] Run `cargo publish --dry-run -p jellyflow-core`.
- [x] Run `cargo publish --dry-run -p jellyflow-runtime`, or record the
      expected unpublished `jellyflow-core` dependency blocker.
- [x] Run `git diff --check`.
- [x] Update task artifacts with validation evidence and unresolved blockers.
- [x] Capture release-readiness conventions in `.trellis/spec`.
- [ ] Commit the completed task with a Conventional Commit message.

## Validation Commands

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

## Risky Files And Rollback Points

- `.github/workflows/ci.yml`: validate syntax carefully; if CI is too broad,
  reduce to the smallest gate set instead of adding release automation.
- `Cargo.toml` and crate manifests: do not change `rust-version`, package name,
  version, or dependency versions without explicit evidence.
- `README.md` and release docs: keep wording factual; avoid claiming crates are
  published or ready if dry-runs expose blockers.

## Follow-Up Checks Before Starting

- Resolve whether this task includes an actual publishing workflow. Recommended:
  no, defer `release-crates.yml` until package dry-runs and first-release order
  are settled.
- Decide CI platform breadth:
  - resolved: Ubuntu CI only for all common gates in this task;
  - follow-up alternative: Ubuntu, macOS, and Windows matrix for Rust tests,
    with smoke scripts on Ubuntu.
