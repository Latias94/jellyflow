from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import math
import os
import sys
import tempfile
import time
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "run_real_window_smoke_matrix.py"
SPEC = importlib.util.spec_from_file_location("run_real_window_smoke_matrix", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {SCRIPT_PATH}")
matrix = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(matrix)


FAKE_SMOKE_SOURCE = r'''from __future__ import annotations

import json
import os
import subprocess
import sys
import time

demo = "workflow" if "--workflow" in sys.argv else "shader"
bounds = os.environ["JELLYFLOW_SMOKE_BOUNDS"]
scenario = os.environ["JELLYFLOW_SMOKE_SCENARIO"]
zoom = os.environ["JELLYFLOW_SMOKE_ZOOM"]
run_id = f"{scenario}-{demo}-{bounds}-z{zoom}"
mode = os.environ.get("FAKE_SMOKE_MODE", "pass")
target = os.environ.get("FAKE_SMOKE_TARGET", "")
active_mode = mode if not target or run_id == target else "pass"

print("JELLYFLOW_REAL_WINDOW_SMOKE_RECEIPT_DIAGNOSTIC ignored=true", file=sys.stderr)
if active_mode in {"timeout", "timeout_child"}:
    if active_mode == "timeout_child":
        sentinel = os.environ["FAKE_SMOKE_SENTINEL"]
        subprocess.Popen(
            [
                sys.executable,
                "-c",
                "import pathlib,time,sys; time.sleep(0.4); pathlib.Path(sys.argv[1]).write_text('leaked')",
                sentinel,
            ]
        )
    time.sleep(5)
elif active_mode == "malformed":
    print("JELLYFLOW_REAL_WINDOW_SMOKE_RECEIPT {not-json")
elif active_mode == "duplicate-key":
    print('JELLYFLOW_REAL_WINDOW_SMOKE_RECEIPT {"schema_version":1,"schema_version":1}')
elif active_mode != "missing":
    receipt_demo = "workflow" if active_mode == "mismatch" and demo == "shader" else demo
    status = "fail" if active_mode in {"zero-fail", "nonzero"} else "pass"
    receipt = {
        "schema_version": 1,
        "status": status,
        "run": {
            "run_id": run_id,
            "demo": receipt_demo,
            "bounds": bounds,
            "scenario": scenario,
            "target_zoom": zoom,
        },
        "failure": {"code": "fixture_failure", "message": None} if status == "fail" else None,
        "evidence": {"paint": {"image_width": 960}} if status == "pass" else None,
        "presented_frames": [],
    }
    if active_mode == "validator-failure":
        receipt["force_validator_failure"] = True
    elif active_mode == "validator-malformed-output":
        receipt["force_malformed_output"] = True
    payload = json.dumps(receipt, indent=2) if active_mode == "pretty" else json.dumps(receipt)
    # A receipt is one physical line by contract; pretty mode intentionally violates it.
    print("JELLYFLOW_REAL_WINDOW_SMOKE_RECEIPT " + payload)
    if active_mode == "duplicate":
        print("JELLYFLOW_REAL_WINDOW_SMOKE_RECEIPT " + json.dumps(receipt), file=sys.stderr)

if active_mode in {"nonzero", "nonzero-pass"}:
    raise SystemExit(7)
'''


FAKE_VALIDATOR_SOURCE = r'''from __future__ import annotations

import json
import sys

if sys.argv[1:] != ["validate"]:
    raise SystemExit(2)
receipt = json.load(sys.stdin)
if receipt.pop("force_validator_failure", False):
    print("fixture rejected receipt", file=sys.stderr)
    raise SystemExit(9)
if receipt.pop("force_malformed_output", False):
    print("{not-json")
else:
    print(json.dumps(receipt, separators=(",", ":")))
'''


def read_results(capture_dir: Path) -> dict[str, object]:
    return json.loads((capture_dir / matrix.RESULTS_NAME).read_text(encoding="utf-8"))


class RealWindowSmokeMatrixTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary_directory.name)
        self.fake_smoke = self.root / "fake_smoke.py"
        self.fake_smoke.write_text(FAKE_SMOKE_SOURCE, encoding="utf-8")
        self.fake_validator = self.root / "fake_validator.py"
        self.fake_validator.write_text(
            "#!/usr/bin/env python3\n" + FAKE_VALIDATOR_SOURCE,
            encoding="utf-8",
        )
        self.fake_validator.chmod(0o755)

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def environment(
        self, *, mode: str = "pass", target: str = "", sentinel: Path | None = None
    ) -> dict[str, str]:
        environment = dict(os.environ)
        environment["FAKE_SMOKE_MODE"] = mode
        environment["FAKE_SMOKE_TARGET"] = target
        if sentinel is not None:
            environment["FAKE_SMOKE_SENTINEL"] = str(sentinel)
        return environment

    def execute(
        self,
        spec: matrix.RunSpec,
        *,
        mode: str,
        timeout_seconds: float = 2.0,
        sentinel: Path | None = None,
    ) -> dict[str, object]:
        capture_dir = self.root / "single"
        capture_dir.mkdir(exist_ok=True)
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
            io.StringIO()
        ):
            return matrix.execute_run(
                spec,
                binary=Path(sys.executable),
                receipt_validator=self.fake_validator,
                binary_arguments=[str(self.fake_smoke)],
                capture_dir=capture_dir,
                timeout_seconds=timeout_seconds,
                base_environment=self.environment(
                    mode=mode, target=spec.run_id, sentinel=sentinel
                ),
            )

    def assert_failure_code(self, run: dict[str, object], code: str) -> None:
        self.assertEqual(run["status"], "fail")
        self.assertIsNone(run["receipt"])
        self.assertTrue(
            any(
                str(error).startswith(f"transport.{code}:")
                for error in run["errors"]
            ),
            run["errors"],
        )

    def test_complete_matrix_writes_nine_canonical_receipts_atomically(self) -> None:
        output_file = self.root / "nested" / "step-output.txt"
        capture_root = self.root / "captures"

        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
            io.StringIO()
        ):
            capture_dir, passed = matrix.run_matrix(
                binary=Path(sys.executable),
                receipt_validator=self.fake_validator,
                binary_arguments=[str(self.fake_smoke)],
                capture_root=capture_root,
                timeout_seconds=2.0,
                capture_dir_output=output_file,
                base_environment=self.environment(),
            )

        self.assertTrue(passed)
        document = read_results(capture_dir)
        self.assertEqual(document["schema_version"], matrix.RESULTS_SCHEMA_VERSION)
        runs = document["runs"]
        self.assertEqual(len(runs), len(matrix.RUN_SPECS))
        self.assertEqual(
            [run["run_id"] for run in runs],
            [spec.run_id for spec in matrix.RUN_SPECS],
        )
        for spec, run in zip(matrix.RUN_SPECS, runs, strict=True):
            self.assertEqual(run["status"], "pass")
            self.assertEqual(run["exit_code"], 0)
            self.assertEqual(run["receipt_count"], 1)
            self.assertEqual(run["receipt"]["run"], spec.receipt_run())
            self.assertEqual(run["errors"], [])
        self.assertEqual(
            output_file.read_text(encoding="utf-8").splitlines(),
            [f"capture_dir={capture_dir}"],
        )
        self.assertEqual(list(capture_dir.glob(".results.json.*.tmp")), [])

    def test_timeout_and_nonzero_exit_are_transport_failures(self) -> None:
        spec = matrix.RUN_SPECS[0]

        timed_out = self.execute(spec, mode="timeout", timeout_seconds=0.05)
        nonzero = self.execute(spec, mode="nonzero")

        self.assertEqual(timed_out["exit_code"], matrix.TIMEOUT_EXIT_CODE)
        self.assertTrue(timed_out["timed_out"])
        self.assert_failure_code(timed_out, "timeout")
        self.assertEqual(nonzero["exit_code"], 7)
        self.assert_failure_code(nonzero, "nonzero_exit")

    @unittest.skipUnless(os.name == "posix", "process-group cleanup is POSIX-specific")
    def test_timeout_terminates_descendant_process_group(self) -> None:
        spec = matrix.RUN_SPECS[0]
        sentinel = self.root / "descendant-leaked"

        run = self.execute(
            spec,
            mode="timeout_child",
            timeout_seconds=0.05,
            sentinel=sentinel,
        )
        time.sleep(0.5)

        self.assert_failure_code(run, "timeout")
        self.assertFalse(sentinel.exists())

    def test_receipt_transport_failures_are_distinct(self) -> None:
        spec = matrix.RUN_SPECS[0]
        cases = (
            ("missing", "missing_receipt", 0),
            ("duplicate", "duplicate_receipt", 2),
            ("malformed", "malformed_receipt", 1),
            ("duplicate-key", "malformed_receipt", 1),
            ("validator-failure", "validator_failure", 1),
            ("validator-malformed-output", "validator_failure", 1),
        )

        for mode, code, count in cases:
            with self.subTest(mode=mode):
                run = self.execute(spec, mode=mode)
                self.assertEqual(run["receipt_count"], count)
                self.assert_failure_code(run, code)

    def test_exit_status_contradictions_are_rejected(self) -> None:
        spec = matrix.RUN_SPECS[0]

        zero_fail = self.execute(spec, mode="zero-fail")
        nonzero_pass = self.execute(spec, mode="nonzero-pass")

        self.assert_failure_code(zero_fail, "exit_status_contradiction")
        self.assert_failure_code(nonzero_pass, "exit_status_contradiction")

    def test_receipt_run_must_match_the_invoked_matrix_cell(self) -> None:
        run = self.execute(matrix.RUN_SPECS[0], mode="mismatch")

        self.assert_failure_code(run, "receipt_run_mismatch")

    def test_failed_run_is_persisted_and_later_matrix_runs_continue(self) -> None:
        failed_spec = matrix.RUN_SPECS[0]
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
            io.StringIO()
        ):
            capture_dir, passed = matrix.run_matrix(
                binary=Path(sys.executable),
                receipt_validator=self.fake_validator,
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
        self.assert_failure_code(runs[0], "nonzero_exit")
        self.assertTrue(all(run["status"] == "pass" for run in runs[1:]))

    def test_only_whole_prefixed_receipt_lines_are_extracted(self) -> None:
        receipt = '{"schema_version":1}'
        stdout = "\n".join(
            (
                f"prefix {matrix.RECEIPT_PREFIX}{receipt}",
                "JELLYFLOW_REAL_WINDOW_SMOKE_RECEIPT_DIAGNOSTIC internal=true",
                f"{matrix.RECEIPT_PREFIX}{receipt}",
            )
        )

        self.assertEqual(matrix.extract_receipt_payloads(stdout, ""), [receipt])

    def test_multiline_json_is_not_accepted_as_a_receipt_line(self) -> None:
        run = self.execute(matrix.RUN_SPECS[0], mode="pretty")

        self.assert_failure_code(run, "malformed_receipt")

    def test_timeout_must_be_positive_and_validator_is_required(self) -> None:
        for timeout in (0.0, -1.0, math.nan, math.inf):
            with self.subTest(timeout=timeout), self.assertRaisesRegex(
                ValueError, "positive and finite"
            ):
                matrix.run_matrix(
                    binary=Path(sys.executable),
                    receipt_validator=self.fake_validator,
                    binary_arguments=[str(self.fake_smoke)],
                    capture_root=self.root / "invalid-timeout",
                    timeout_seconds=timeout,
                    base_environment=self.environment(),
                )

        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            matrix.parse_args([])
        parsed = matrix.parse_args(
            ["--receipt-validator", str(self.fake_validator), "--timeout-seconds", "1"]
        )
        self.assertEqual(parsed.receipt_validator, self.fake_validator)

if __name__ == "__main__":
    unittest.main()
