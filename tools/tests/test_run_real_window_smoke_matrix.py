from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import math
import os
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "run_real_window_smoke_matrix.py"
SPEC = importlib.util.spec_from_file_location("run_real_window_smoke_matrix", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {SCRIPT_PATH}")
matrix = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(matrix)


FAKE_SMOKE_SOURCE = r'''from __future__ import annotations

import os
import sys
import time


demo = "workflow" if "--workflow" in sys.argv else "shader"
bounds = os.environ["JELLYFLOW_SMOKE_BOUNDS"]
scenario = os.environ["JELLYFLOW_SMOKE_SCENARIO"]
zoom = os.environ["JELLYFLOW_SMOKE_ZOOM"]
run_id = f"{scenario}-{demo}-{bounds}-z{zoom}"
mode = os.environ.get("FAKE_SMOKE_MODE", "pass")
target = os.environ.get("FAKE_SMOKE_TARGET", "")
active_mode = mode if run_id == target else "pass"

print("JELLYFLOW_REAL_WINDOW_SMOKE_CONNECT_PRESS internal=true", file=sys.stderr)
if active_mode == "timeout":
    time.sleep(5)
elif active_mode != "missing":
    marker_demo = "workflow" if active_mode == "mismatch" and demo == "shader" else demo
    marker = (
        "JELLYFLOW_REAL_WINDOW_SMOKE "
        f"status=pass demo={marker_demo} bounds={bounds} scenario={scenario} "
        f"target_zoom={zoom} diagnostic=None"
    )
    print(marker)
    if active_mode == "duplicate":
        print(marker, file=sys.stderr)

if active_mode == "nonzero":
    raise SystemExit(7)
'''


def read_results(capture_dir: Path) -> dict[str, object]:
    return json.loads(
        (capture_dir / matrix.RESULTS_NAME).read_text(encoding="utf-8")
    )


class RealWindowSmokeMatrixTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary_directory.name)
        self.fake_smoke = self.root / "fake_smoke.py"
        self.fake_smoke.write_text(FAKE_SMOKE_SOURCE, encoding="utf-8")

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def environment(self, *, mode: str = "pass", target: str = "") -> dict[str, str]:
        environment = dict(os.environ)
        environment["FAKE_SMOKE_MODE"] = mode
        environment["FAKE_SMOKE_TARGET"] = target
        return environment

    def execute(
        self,
        spec: matrix.RunSpec,
        *,
        mode: str,
        timeout_seconds: float = 2.0,
    ) -> dict[str, object]:
        capture_dir = self.root / "single"
        capture_dir.mkdir(exist_ok=True)
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
            io.StringIO()
        ):
            return matrix.execute_run(
                spec,
                binary=Path(sys.executable),
                binary_arguments=[str(self.fake_smoke)],
                capture_dir=capture_dir,
                timeout_seconds=timeout_seconds,
                base_environment=self.environment(mode=mode, target=spec.run_id),
            )

    def test_complete_matrix_writes_nine_pass_receipts_in_a_unique_child(self) -> None:
        output_file = self.root / "nested" / "step-output.txt"
        capture_root = self.root / "captures"

        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
            io.StringIO()
        ):
            first_dir, first_passed = matrix.run_matrix(
                binary=Path(sys.executable),
                binary_arguments=[str(self.fake_smoke)],
                capture_root=capture_root,
                timeout_seconds=2.0,
                capture_dir_output=output_file,
                base_environment=self.environment(),
            )
            second_dir, second_passed = matrix.run_matrix(
                binary=Path(sys.executable),
                binary_arguments=[str(self.fake_smoke)],
                capture_root=capture_root,
                timeout_seconds=2.0,
                capture_dir_output=output_file,
                base_environment=self.environment(),
            )

        self.assertTrue(first_passed)
        self.assertTrue(second_passed)
        self.assertNotEqual(first_dir, second_dir)
        self.assertEqual(first_dir.parent, capture_root.resolve())
        document = read_results(first_dir)
        self.assertEqual(document["schema_version"], matrix.RESULTS_SCHEMA_VERSION)
        runs = document["runs"]
        self.assertEqual(len(runs), 9)
        self.assertEqual(
            [run["run_id"] for run in runs],
            [spec.run_id for spec in matrix.RUN_SPECS],
        )
        self.assertEqual(
            sum(spec.scenario == "interaction" for spec in matrix.RUN_SPECS), 8
        )
        self.assertEqual(
            sum(spec.scenario == "lod-threshold" for spec in matrix.RUN_SPECS), 1
        )
        for run in runs:
            self.assertEqual(run["status"], "pass")
            self.assertEqual(run["exit_code"], 0)
            self.assertEqual(run["marker_count"], 1)
            self.assertEqual(run["errors"], [])
            self.assertIn(
                "JELLYFLOW_REAL_WINDOW_SMOKE_CONNECT_PRESS", run["stderr"]
            )
        self.assertEqual(
            output_file.read_text(encoding="utf-8").splitlines(),
            [f"capture_dir={first_dir}", f"capture_dir={second_dir}"],
        )
        self.assertEqual(list(first_dir.glob(".results.json.*.tmp")), [])

    def test_timeout_and_nonzero_exit_are_named_failures(self) -> None:
        spec = matrix.RUN_SPECS[0]

        timed_out = self.execute(spec, mode="timeout", timeout_seconds=0.05)
        nonzero = self.execute(spec, mode="nonzero")

        self.assertEqual(timed_out["run_id"], spec.run_id)
        self.assertEqual(timed_out["exit_code"], matrix.TIMEOUT_EXIT_CODE)
        self.assertTrue(timed_out["timed_out"])
        self.assertEqual(timed_out["status"], "fail")
        self.assertIn("run timed out", timed_out["errors"])
        self.assertEqual(nonzero["exit_code"], 7)
        self.assertEqual(nonzero["marker_count"], 1)
        self.assertEqual(nonzero["status"], "fail")
        self.assertIn("exit code is 7, expected 0", nonzero["errors"])

    def test_failed_run_is_persisted_and_later_matrix_runs_continue(self) -> None:
        failed_spec = matrix.RUN_SPECS[0]
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
            io.StringIO()
        ):
            capture_dir, passed = matrix.run_matrix(
                binary=Path(sys.executable),
                binary_arguments=[str(self.fake_smoke)],
                capture_root=self.root / "failed-matrix",
                timeout_seconds=2.0,
                base_environment=self.environment(
                    mode="nonzero", target=failed_spec.run_id
                ),
            )

        self.assertFalse(passed)
        runs = read_results(capture_dir)["runs"]
        self.assertEqual(len(runs), len(matrix.RUN_SPECS))
        self.assertEqual(runs[0]["run_id"], failed_spec.run_id)
        self.assertEqual(runs[0]["status"], "fail")
        self.assertEqual(runs[0]["exit_code"], 7)
        self.assertTrue(all(run["status"] == "pass" for run in runs[1:]))

    def test_missing_duplicate_and_mismatched_markers_are_rejected(self) -> None:
        spec = matrix.RUN_SPECS[0]

        missing = self.execute(spec, mode="missing")
        duplicate = self.execute(spec, mode="duplicate")
        mismatch = self.execute(spec, mode="mismatch")

        self.assertEqual(missing["marker_count"], 0)
        self.assertIn(
            "final marker count is 0, expected exactly 1", missing["errors"]
        )
        self.assertEqual(duplicate["marker_count"], 2)
        self.assertIsNone(duplicate["marker"])
        self.assertIn(
            "final marker count is 2, expected exactly 1", duplicate["errors"]
        )
        self.assertEqual(mismatch["marker_count"], 1)
        self.assertIn(
            "marker demo is 'workflow', expected 'shader'", mismatch["errors"]
        )

    def test_only_whole_final_marker_lines_are_counted(self) -> None:
        final = (
            "JELLYFLOW_REAL_WINDOW_SMOKE status=pass demo=shader bounds=compact "
            "scenario=interaction target_zoom=1"
        )
        stdout = "\n".join(
            (
                "prefix JELLYFLOW_REAL_WINDOW_SMOKE status=pass",
                "JELLYFLOW_REAL_WINDOW_SMOKE_CONNECT_PRESS internal=true",
                final,
            )
        )

        self.assertEqual(matrix.extract_final_marker_lines(stdout, ""), [final])

    def test_timeout_must_be_positive_and_finite(self) -> None:
        for timeout in (0.0, -1.0, math.nan, math.inf):
            with self.subTest(timeout=timeout), self.assertRaisesRegex(
                ValueError, "positive and finite"
            ):
                matrix.run_matrix(
                    binary=Path(sys.executable),
                    binary_arguments=[str(self.fake_smoke)],
                    capture_root=self.root / "invalid-timeout",
                    timeout_seconds=timeout,
                    base_environment=self.environment(),
                )

        for value in ("0", "-1", "nan", "inf"):
            with self.subTest(value=value), contextlib.redirect_stderr(
                io.StringIO()
            ), self.assertRaises(SystemExit):
                matrix.parse_args(["--timeout-seconds", value])

    def test_workflows_use_orchestrator_output_for_validation_and_upload(self) -> None:
        workflow_directory = SCRIPT_PATH.parents[1] / ".github" / "workflows"
        workflows = {
            "ci": (workflow_directory / "ci.yml").read_text(encoding="utf-8"),
            "release": (workflow_directory / "release-preflight.yml").read_text(
                encoding="utf-8"
            ),
        }

        for workflow in workflows.values():
            self.assertIn("python3 tools/run_real_window_smoke_matrix.py", workflow)
            self.assertIn(
                "steps.real_window_smoke_matrix.outputs.capture_dir", workflow
            )
            self.assertIn("/results.json", workflow)
            self.assertNotIn("run_smoke()", workflow)
        self.assertIn("Archived Open GPUI package consumer", workflows["release"])
        self.assertIn("timeout-minutes: 60", workflows["release"])


if __name__ == "__main__":
    unittest.main()
