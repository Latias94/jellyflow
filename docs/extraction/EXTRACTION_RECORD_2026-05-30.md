# Jellyflow Extraction Record

Date: 2026-05-30
Status: Local extraction created

## Source

- Source repo: `/Users/frankorz/codes/rust/fret`
- Source commit: `fc0e4532139d1a154913e56d4f2fa0bc7562ea6b`
- Filtered HEAD before this record commit: `b61e4e3ee55094fa149ed0bcd91abf904a944539`
- Target repo: `/Users/frankorz/codes/rust/jellyflow`

## Command

```bash
git clone --no-hardlinks /Users/frankorz/codes/rust/fret /Users/frankorz/codes/rust/jellyflow
cd /Users/frankorz/codes/rust/jellyflow
SOURCE_COMMIT="$(git rev-parse HEAD)"
printf '%s\n' "$SOURCE_COMMIT" > JELLYFLOW_SOURCE_COMMIT.txt
git filter-repo --analyze
git filter-repo --force \
  --paths-from-file /Users/frankorz/codes/rust/fret/docs/workstreams/jellyflow-repo-extraction-v1/jellyflow-filter-paths.txt \
  --path-rename ecosystem/jellyflow-core/:crates/jellyflow-core/ \
  --path-rename ecosystem/jellyflow-runtime/:crates/jellyflow-runtime/ \
  --path-rename docs/workstreams/jellyflow-package-split-v1/:docs/history/fret-workstreams/jellyflow-package-split-v1/ \
  --path-rename docs/workstreams/jellyflow-standalone-readiness-v1/:docs/history/fret-workstreams/jellyflow-standalone-readiness-v1/ \
  --path-rename tools/check_jellyflow_external_smoke.py:tools/check_external_consumer_smoke.py
```

## Validation

Representative history checks passed:

- `git log --follow -- crates/jellyflow-core/src/core/mod.rs`
- `git log --follow -- crates/jellyflow-core/src/ops/mod.rs`
- `git log --follow -- crates/jellyflow-runtime/src/runtime/store.rs`
- `git log --follow -- crates/jellyflow-runtime/src/io/mod.rs`

Excluded path check passed:

```bash
find . -path './.git' -prune -o \
  \( -path './ecosystem/fret-node/src/ui*' \
  -o -path './crates/fret-*' \
  -o -path './crates/fret-core*' \
  -o -path './crates/fret-ui*' \
  -o -path './ecosystem/fret-canvas*' \
  -o -path './apps*' \) -print
```

The command printed nothing.

## Known Bootstrap Cleanup

The filtered tree still contains `ecosystem/fret-node/src/*/mod.rs` compatibility wrapper remnants
because historical `fret-node/src/*` paths were kept as history inputs. Remove those remnants during
standalone workspace bootstrap.

No crates were published during extraction.
