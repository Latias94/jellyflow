from __future__ import annotations

import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path
from types import ModuleType


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "release-version.py"


def load_release_version_module() -> ModuleType:
    spec = importlib.util.spec_from_file_location("jellyflow_release_version", SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


release_version = load_release_version_module()
CURRENT_VERSION = release_version.cargo_workspace_version()


class ReleaseVersionTests(unittest.TestCase):
    def test_extract_release_notes_requires_one_exact_non_empty_section(self) -> None:
        changelog = """# Changelog

## [Unreleased]

## [0.4.0] - 2026-07-27

### Added

- Added the release gate.

## [0.3.0] - 2026-07-07

- Previous release.
"""

        self.assertEqual(
            release_version.extract_release_notes(changelog, "0.4.0"),
            "### Added\n\n- Added the release gate.",
        )

    def test_extract_release_notes_rejects_missing_section(self) -> None:
        with self.assertRaisesRegex(ValueError, "missing an exact"):
            release_version.extract_release_notes("## [Unreleased]\n", "0.4.0")

    def test_extract_release_notes_rejects_empty_section(self) -> None:
        changelog = "## [0.4.0] - 2026-07-27\n\n## [0.3.0]\n- old\n"

        with self.assertRaisesRegex(ValueError, "section is empty"):
            release_version.extract_release_notes(changelog, "0.4.0")

    def test_extract_release_notes_rejects_duplicate_section(self) -> None:
        changelog = """## [0.4.0]
- First.
## [0.4.0] - 2026-07-27
- Second.
"""

        with self.assertRaisesRegex(ValueError, "duplicate"):
            release_version.extract_release_notes(changelog, "0.4.0")

    def test_current_candidate_has_matching_versions_and_release_notes(self) -> None:
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "check", "--version", CURRENT_VERSION],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(f"CHANGELOG release section: {CURRENT_VERSION}", result.stdout)

    def test_release_notes_command_emits_section_body_without_heading(self) -> None:
        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "release-notes",
                "--version",
                CURRENT_VERSION,
            ],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("### Breaking Changes", result.stdout)
        self.assertNotIn(f"## [{CURRENT_VERSION}]", result.stdout)
        self.assertNotIn("## [0.3.0]", result.stdout)


if __name__ == "__main__":
    unittest.main()
