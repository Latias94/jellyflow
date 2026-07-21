from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "release-crates.yml"
PREFLIGHT_WORKFLOW = ROOT / ".github" / "workflows" / "release-preflight.yml"
RELEASE_TRUST_SCRIPT = ROOT / "scripts" / "verify-release-source-trust.sh"
RELEASE_TAG_SCRIPT = ROOT / "scripts" / "verify-release-tag-unchanged.sh"
RELEASE_CRATES = ROOT / "scripts" / "release-crates.txt"
RELEASE_VERSION_SCRIPT = ROOT / "scripts" / "release-version.py"
ROOT_MANIFEST = ROOT / "Cargo.toml"


def workflow_source(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def trigger_source(source: str) -> str:
    trigger, separator, _ = source.partition("\npermissions:")
    if not separator:
        raise AssertionError("workflow is missing its top-level permissions block")
    return trigger


def job_source(source: str, job: str, next_job: str | None = None) -> str:
    marker = f"  {job}:\n"
    _, separator, tail = source.partition(marker)
    if not separator:
        raise AssertionError(f"workflow is missing the {job!r} job")
    if next_job is None:
        return tail
    block, separator, _ = tail.partition(f"  {next_job}:\n")
    if not separator:
        raise AssertionError(f"workflow is missing the {next_job!r} job")
    return block


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
case "$*" in
  *"/branches/"*) printf '%s\n' "${STUB_BRANCH_PROTECTED:?}" ;;
  *"/actions/workflows/ci.yml/runs"*) printf '%s\n' "${STUB_SUCCESSFUL_RUNS:?}" ;;
  *) exit 64 ;;
esac
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
    def test_publication_uses_a_default_branch_repository_dispatch(self) -> None:
        release = workflow_source(RELEASE_WORKFLOW)
        preflight = workflow_source(PREFLIGHT_WORKFLOW)

        self.assertNotIn("\n  push:", trigger_source(release))
        self.assertNotIn("workflow_dispatch:", trigger_source(release))
        self.assertIn("repository_dispatch:", trigger_source(release))
        self.assertIn("types: [release-crates]", trigger_source(release))
        self.assertNotIn("workflow_dispatch:", trigger_source(preflight))
        self.assertIn("workflow_call:", trigger_source(preflight))

    def test_release_source_is_resolved_from_an_untrusted_tag_payload(self) -> None:
        release = workflow_source(RELEASE_WORKFLOW)
        resolve = job_source(release, "resolve", "preflight")

        self.assertIn(
            "RELEASE_TAG: ${{ github.event.client_payload.release_tag }}", resolve
        )
        self.assertIn("TRUSTED_WORKFLOW_SHA: ${{ github.sha }}", resolve)
        self.assertIn("ref: ${{ github.sha }}", resolve)
        self.assertIn(
            'source_sha="$(git rev-parse "refs/release-tags/$RELEASE_TAG^{commit}")"',
            resolve,
        )
        self.assertIn(
            'git merge-base --is-ancestor "$source_sha" "$TRUSTED_WORKFLOW_SHA"',
            resolve,
        )
        self.assertNotIn("source_ref=refs/tags/", resolve)

    def test_preflight_accepts_and_checks_out_only_an_exact_commit_sha(self) -> None:
        release = workflow_source(RELEASE_WORKFLOW)
        preflight = workflow_source(PREFLIGHT_WORKFLOW)

        self.assertNotIn("source_ref:", preflight)
        self.assertIn("source_sha:", preflight)
        self.assertEqual(preflight.count("ref: ${{ inputs.source_sha }}"), 2)
        self.assertIn(
            "source_sha: ${{ needs.resolve.outputs.source_sha }}",
            job_source(release, "preflight", "publish"),
        )

    def test_release_requires_a_protected_default_branch_and_exact_ci(self) -> None:
        release = workflow_source(RELEASE_WORKFLOW)
        resolve = job_source(release, "resolve", "preflight")
        trust_script = workflow_source(RELEASE_TRUST_SCRIPT)

        self.assertIn(
            "DEFAULT_BRANCH: ${{ github.event.repository.default_branch }}", resolve
        )
        self.assertIn("run: bash scripts/verify-release-source-trust.sh", resolve)
        self.assertIn("--jq '.protected'", trust_script)
        self.assertIn('-f head_sha="$SOURCE_SHA"', trust_script)
        self.assertIn('-f branch="$DEFAULT_BRANCH"', trust_script)
        self.assertIn("-f event=push", trust_script)
        self.assertIn("-f status=success", trust_script)

    def test_unprotected_default_branch_is_rejected(self) -> None:
        result = run_release_trust_script(branch_protected="false")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("is not protected", result.stdout + result.stderr)

    def test_source_without_successful_exact_ci_is_rejected(self) -> None:
        result = run_release_trust_script(successful_runs="0")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("No successful CI push run", result.stdout + result.stderr)

    def test_invalid_ci_response_is_rejected(self) -> None:
        result = run_release_trust_script(successful_runs="null")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("invalid successful CI run count", result.stdout + result.stderr)

    def test_protected_source_with_successful_exact_ci_is_accepted(self) -> None:
        result = run_release_trust_script()

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_remote_tag_object_cannot_change_during_release(self) -> None:
        release = workflow_source(RELEASE_WORKFLOW)
        resolve = job_source(release, "resolve", "preflight")
        publish = job_source(release, "publish", "github-release")
        github_release = job_source(release, "github-release")
        tag_script = workflow_source(RELEASE_TAG_SCRIPT)

        self.assertIn("tag_oid: ${{ steps.source.outputs.tag_oid }}", resolve)
        self.assertIn('echo "tag_oid=$tag_oid"', resolve)
        for privileged_job in (publish, github_release):
            self.assertIn(
                "TAG_OID: ${{ needs.resolve.outputs.tag_oid }}", privileged_job
            )
            self.assertIn("bash scripts/verify-release-tag-unchanged.sh", privileged_job)
        self.assertIn(
            'if [ "$tag_oid" != "$expected_tag_oid" ]; then', tag_script
        )
        self.assertIn(
            'if [ "$tag_sha" != "$expected_source_sha" ]; then', tag_script
        )

    def test_release_tag_revalidation_accepts_an_unchanged_tag(self) -> None:
        result = run_release_tag_script()

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_release_tag_revalidation_rejects_moved_tag_object(self) -> None:
        result = run_release_tag_script(current_tag_oid="c" * 40)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("object moved", result.stdout + result.stderr)

    def test_release_tag_revalidation_rejects_moved_tag_commit(self) -> None:
        result = run_release_tag_script(current_source_sha="d" * 40)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("moved", result.stdout + result.stderr)

    def test_release_crate_order_has_one_shared_source(self) -> None:
        crates = RELEASE_CRATES.read_text(encoding="utf-8").splitlines()
        self.assertEqual(
            crates,
            [
                "jellyflow-core",
                "jellyflow-layout",
                "jellyflow-runtime",
                "jellyflow-egui",
                "jellyflow-open-gpui",
            ],
        )
        for workflow in (CI_WORKFLOW, RELEASE_WORKFLOW, PREFLIGHT_WORKFLOW):
            self.assertIn(
                "mapfile -t crates < scripts/release-crates.txt",
                workflow_source(workflow),
            )

    def test_release_version_rejects_a_removed_package_cleanly(self) -> None:
        result = subprocess.run(
            [
                sys.executable,
                str(RELEASE_VERSION_SCRIPT),
                "crate-version",
                "--crate",
                "removed-package",
            ],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("::error::", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_root_workspace_contains_only_the_release_crates(self) -> None:
        manifest = tomllib.loads(ROOT_MANIFEST.read_text(encoding="utf-8"))
        members = manifest["workspace"]["members"]
        expected = [f"crates/{crate}" for crate in RELEASE_CRATES.read_text(encoding="utf-8").splitlines()]

        self.assertEqual(set(members), set(expected))
        self.assertEqual(len(members), len(expected))

    def test_privileged_release_jobs_share_the_protected_environment(self) -> None:
        release = workflow_source(RELEASE_WORKFLOW)
        publish = job_source(release, "publish", "github-release")
        github_release = job_source(release, "github-release")

        self.assertIn("environment: crates.io", publish)
        self.assertIn("environment: crates.io", github_release)
        self.assertIn('--target "$SOURCE_SHA"', github_release)

    def test_github_release_query_cannot_treat_api_failures_as_absence(self) -> None:
        release = workflow_source(RELEASE_WORKFLOW)
        github_release = job_source(release, "github-release")

        self.assertNotIn("gh release view", github_release)
        self.assertIn("gh api graphql", github_release)
        self.assertIn("release(tagName: $tagName) { id }", github_release)
        self.assertIn(
            '.data.repository.release == null then "missing"', github_release
        )
        self.assertIn('else "existing" end', github_release)
        self.assertIn("for attempt in 1 2 3; do", github_release)
        self.assertIn("is_transient_release_query_failure", github_release)
        self.assertIn("refusing to infer that it is absent", github_release)
        self.assertIn('release_state="$(query_github_release)"', github_release)
        self.assertIn('case "$release_state" in', github_release)
        self.assertLess(
            github_release.index("\n            missing)"),
            github_release.index('gh release create "$tag"'),
        )


if __name__ == "__main__":
    unittest.main()
