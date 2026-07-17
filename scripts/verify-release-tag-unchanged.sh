#!/usr/bin/env bash
# GitHub Actions injects the uppercase inputs; the validation loop below rejects missing values.
# shellcheck disable=SC2153

set -euo pipefail

for required in RELEASE_TAG SOURCE_SHA TAG_OID; do
  if [ -z "${!required:-}" ]; then
    echo "::error::Required release tag input $required is missing."
    exit 1
  fi
done

release_tag="$RELEASE_TAG"
expected_source_sha="$SOURCE_SHA"
expected_tag_oid="$TAG_OID"
release_stage="${RELEASE_STAGE:-publication}"
timeout --signal=TERM --kill-after=10s 120s \
  git fetch --force --no-tags origin \
    "+refs/tags/$release_tag:refs/release-tags/$release_tag"
tag_oid="$(git rev-parse "refs/release-tags/$release_tag")"
tag_sha="$(git rev-parse "refs/release-tags/$release_tag^{commit}")"

if [ "$tag_oid" != "$expected_tag_oid" ]; then
  echo "::error::Release tag $release_tag object moved from $expected_tag_oid to $tag_oid during $release_stage."
  exit 1
fi
if [ "$tag_sha" != "$expected_source_sha" ]; then
  echo "::error::Release tag $release_tag moved from $expected_source_sha to $tag_sha during $release_stage."
  exit 1
fi
