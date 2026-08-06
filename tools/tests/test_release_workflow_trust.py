from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
RELEASE_TRUST_SCRIPT = ROOT / "scripts" / "verify-release-source-trust.sh"
RELEASE_TAG_SCRIPT = ROOT / "scripts" / "verify-release-tag-unchanged.sh"


def write_executable(path: Path, source: str) -> None:
    path.write_text(source, encoding="utf-8")
    path.chmod(0o755)


def run_release_trust_script(
    *, branch_protected: str = "true", successful_runs: str = "1"
) -> subprocess.CompletedProcess[str]:
    with tempfile.TemporaryDirectory() as temporary_directory:
        bin_directory = Path(temporary_directory)
        write_executable(
            bin_directory / "timeout",
            """#!/usr/bin/env bash
set -euo pipefail
while [ "$#" -gt 0 ]; do
  case "$1" in
    --signal=*|--kill-after=*) shift ;;
    *s) shift; break ;;
    *) break ;;
  esac
done
exec "$@"
""",
        )
        write_executable(
            bin_directory / "jq",
            """#!/usr/bin/env bash
set -euo pipefail
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--arg" ] && [ "${2:-}" = "branch" ]; then
    printf '%s\n' "${3:-}"
    exit 0
  fi
  shift
done
exit 64
""",
        )
        write_executable(
            bin_directory / "gh",
            """#!/usr/bin/env bash
set -euo pipefail
actual=("$@")
case "${4:-}" in
  "repos/${GITHUB_REPOSITORY:?}/branches/${DEFAULT_BRANCH:?}")
    expected=(
      api -X GET
      "repos/$GITHUB_REPOSITORY/branches/$DEFAULT_BRANCH"
      --jq .protected
    )
    output="${STUB_BRANCH_PROTECTED:?}"
    ;;
  "repos/${GITHUB_REPOSITORY:?}/actions/workflows/ci.yml/runs")
    expected=(
      api -X GET
      "repos/$GITHUB_REPOSITORY/actions/workflows/ci.yml/runs"
      -f "head_sha=$SOURCE_SHA"
      -f "branch=$DEFAULT_BRANCH"
      -f event=push
      -f status=success
      -f per_page=100
      --jq .total_count
    )
    output="${STUB_SUCCESSFUL_RUNS:?}"
    ;;
  *)
    printf 'unexpected gh endpoint: %q\n' "${4:-}" >&2
    exit 64
    ;;
esac
if [ "${#actual[@]}" -ne "${#expected[@]}" ]; then
  printf 'unexpected gh argument count: %s\n' "${actual[*]}" >&2
  exit 64
fi
for index in "${!expected[@]}"; do
  if [ "${actual[$index]}" != "${expected[$index]}" ]; then
    printf 'unexpected gh arguments: %s\n' "${actual[*]}" >&2
    exit 64
  fi
done
printf '%s\n' "$output"
""",
        )
        environment = os.environ.copy()
        environment.update(
            {
                "DEFAULT_BRANCH": "main",
                "GH_TOKEN": "test-token",
                "GITHUB_REPOSITORY": "example/jellyflow",
                "PATH": f"{bin_directory}{os.pathsep}{environment['PATH']}",
                "SOURCE_SHA": "a" * 40,
                "STUB_BRANCH_PROTECTED": branch_protected,
                "STUB_SUCCESSFUL_RUNS": successful_runs,
            }
        )
        return subprocess.run(
            ["bash", str(RELEASE_TRUST_SCRIPT)],
            cwd=ROOT,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
        )


def run_release_tag_script(
    *, current_tag_oid: str | None = None, current_source_sha: str | None = None
) -> subprocess.CompletedProcess[str]:
    expected_tag_oid = "b" * 40
    expected_source_sha = "a" * 40
    with tempfile.TemporaryDirectory() as temporary_directory:
        bin_directory = Path(temporary_directory)
        write_executable(
            bin_directory / "timeout",
            """#!/usr/bin/env bash
set -euo pipefail
while [ "$#" -gt 0 ]; do
  case "$1" in
    --signal=*|--kill-after=*) shift ;;
    *s) shift; break ;;
    *) break ;;
  esac
done
exec "$@"
""",
        )
        write_executable(
            bin_directory / "git",
            """#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  fetch) exit 0 ;;
  rev-parse)
    if [[ "${2:-}" == *"^{commit}" ]]; then
      printf '%s\n' "${STUB_SOURCE_SHA:?}"
    else
      printf '%s\n' "${STUB_TAG_OID:?}"
    fi
    ;;
  *) exit 64 ;;
esac
""",
        )
        environment = os.environ.copy()
        environment.update(
            {
                "PATH": f"{bin_directory}{os.pathsep}{environment['PATH']}",
                "RELEASE_TAG": "v0.4.0",
                "SOURCE_SHA": expected_source_sha,
                "STUB_SOURCE_SHA": current_source_sha or expected_source_sha,
                "STUB_TAG_OID": current_tag_oid or expected_tag_oid,
                "TAG_OID": expected_tag_oid,
            }
        )
        return subprocess.run(
            ["bash", str(RELEASE_TAG_SCRIPT)],
            cwd=ROOT,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
        )


class ReleaseWorkflowTrustTests(unittest.TestCase):
    def test_unprotected_default_branch_is_rejected(self) -> None:
        result = run_release_trust_script(branch_protected="false")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("is not protected", result.stdout + result.stderr)

    def test_missing_or_invalid_exact_ci_is_rejected(self) -> None:
        for successful_runs, diagnostic in (
            ("0", "No successful CI push run"),
            ("null", "invalid successful CI run count"),
        ):
            with self.subTest(successful_runs=successful_runs):
                result = run_release_trust_script(successful_runs=successful_runs)

                self.assertNotEqual(result.returncode, 0)
                self.assertIn(diagnostic, result.stdout + result.stderr)

    def test_protected_source_with_successful_exact_ci_is_accepted(self) -> None:
        result = run_release_trust_script()

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_release_tag_revalidation_accepts_an_unchanged_tag(self) -> None:
        result = run_release_tag_script()

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_release_tag_revalidation_rejects_moved_object_or_commit(self) -> None:
        for arguments, diagnostic in (
            ({"current_tag_oid": "c" * 40}, "object moved"),
            ({"current_source_sha": "d" * 40}, "moved"),
        ):
            with self.subTest(arguments=arguments):
                result = run_release_tag_script(**arguments)

                self.assertNotEqual(result.returncode, 0)
                self.assertIn(diagnostic, result.stdout + result.stderr)

if __name__ == "__main__":
    unittest.main()
