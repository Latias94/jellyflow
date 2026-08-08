#!/usr/bin/env bash

set -euo pipefail

for required in DEFAULT_BRANCH GH_TOKEN GITHUB_REPOSITORY SOURCE_SHA; do
  if [ -z "${!required:-}" ]; then
    echo "::error::Required release trust input $required is missing."
    exit 1
  fi
done

if [[ ! "$SOURCE_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  echo "::error::Release source must be a full Git commit SHA."
  exit 1
fi

default_branch_path="$(jq -rn --arg branch "$DEFAULT_BRANCH" '$branch | @uri')"
branch_protected="$(
  timeout --signal=TERM --kill-after=10s 120s gh api -X GET \
    "repos/$GITHUB_REPOSITORY/branches/$default_branch_path" \
    --jq '.protected'
)"
if [ "$branch_protected" != "true" ]; then
  echo "::error::Default branch $DEFAULT_BRANCH is not protected."
  exit 1
fi

successful_runs="$(
  timeout --signal=TERM --kill-after=10s 120s gh api -X GET \
    "repos/$GITHUB_REPOSITORY/actions/workflows/ci.yml/runs" \
    -f head_sha="$SOURCE_SHA" \
    -f branch="$DEFAULT_BRANCH" \
    -f event=push \
    -f status=success \
    -f per_page=100 \
    --jq '.total_count'
)"
if [[ ! "$successful_runs" =~ ^[0-9]+$ ]]; then
  echo "::error::GitHub returned an invalid successful CI run count."
  exit 1
fi
if (( 10#$successful_runs < 1 )); then
  echo "::error::No successful CI push run exists for release commit $SOURCE_SHA."
  exit 1
fi
