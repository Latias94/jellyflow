from __future__ import annotations

import contextlib
import importlib.util
import io
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "run_with_timeout.py"
SPEC = importlib.util.spec_from_file_location("run_with_timeout", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {SCRIPT_PATH}")
timeout_runner = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = timeout_runner
SPEC.loader.exec_module(timeout_runner)


class RunWithTimeoutTests(unittest.TestCase):
    def test_passes_stdin_environment_and_cwd_to_the_child(self) -> None:
        environment = dict(os.environ)
        environment["JELLYFLOW_TIMEOUT_TEST"] = "explicit"
        with tempfile.TemporaryDirectory() as directory:
            completed = timeout_runner.run_process(
                [
                    sys.executable,
                    "-c",
                    "import os, sys; print(os.environ['JELLYFLOW_TIMEOUT_TEST']); "
                    "print(os.path.basename(os.getcwd())); print(sys.stdin.read())",
                ],
                timeout_seconds=5,
                cwd=directory,
                env=environment,
                input="receipt",
                check=True,
                capture_output=True,
                text=True,
            )

        self.assertEqual(completed.returncode, 0)
        self.assertEqual(
            completed.stdout.splitlines(),
            ["explicit", Path(directory).name, "receipt"],
        )

    def test_returns_the_child_exit_code(self) -> None:
        self.assertEqual(
            timeout_runner.run_with_timeout(
                [sys.executable, "-c", "raise SystemExit(7)"],
                timeout_seconds=5,
            ),
            7,
        )

    def test_timeout_returns_124(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()):
            exit_code = timeout_runner.run_with_timeout(
                [sys.executable, "-c", "import time; time.sleep(60)"],
                timeout_seconds=0.1,
                kill_grace_seconds=0.5,
            )

        self.assertEqual(exit_code, timeout_runner.TIMEOUT_EXIT_CODE)

    @unittest.skipUnless(timeout_runner.os.name == "posix", "POSIX process groups only")
    def test_timeout_signals_and_terminates_descendants_in_the_process_group(self) -> None:
        child = """
import pathlib
import signal
import sys
import time

def terminate(*_):
    pathlib.Path(sys.argv[2]).write_text("terminated", encoding="utf-8")
    raise SystemExit(0)

signal.signal(signal.SIGTERM, terminate)
pathlib.Path(sys.argv[1]).write_text("ready", encoding="utf-8")
while True:
    time.sleep(1)
"""
        parent = """
import pathlib
import signal
import subprocess
import sys
import time

signal.signal(signal.SIGTERM, signal.SIG_IGN)
subprocess.Popen([sys.executable, "-c", sys.argv[1], sys.argv[2], sys.argv[3]])
deadline = time.monotonic() + 5
while not pathlib.Path(sys.argv[2]).exists() and time.monotonic() < deadline:
    time.sleep(0.01)
if not pathlib.Path(sys.argv[2]).exists():
    raise SystemExit("descendant did not start")
while True:
    time.sleep(1)
"""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            ready = root / "ready"
            terminated = root / "terminated"
            with self.assertRaises(subprocess.TimeoutExpired):
                timeout_runner.run_process(
                    [
                        sys.executable,
                        "-c",
                        parent,
                        child,
                        str(ready),
                        str(terminated),
                    ],
                    timeout_seconds=1,
                    kill_grace_seconds=0.5,
                )

            self.assertTrue(ready.is_file())
            self.assertEqual(terminated.read_text(encoding="utf-8"), "terminated")

    @unittest.skipUnless(timeout_runner.os.name == "posix", "POSIX process groups only")
    def test_grace_period_tracks_descendants_after_the_group_leader_exits(self) -> None:
        child = """
import pathlib
import signal
import sys
import time

def terminate(*_):
    time.sleep(0.15)
    pathlib.Path(sys.argv[1]).write_text("completed", encoding="utf-8")
    raise SystemExit(0)

signal.signal(signal.SIGTERM, terminate)
pathlib.Path(sys.argv[2]).write_text("ready", encoding="utf-8")
while True:
    time.sleep(1)
"""
        parent = """
import pathlib
import subprocess
import sys
import time

subprocess.Popen([sys.executable, "-c", sys.argv[1], sys.argv[2], sys.argv[3]])
deadline = time.monotonic() + 5
while not pathlib.Path(sys.argv[3]).exists() and time.monotonic() < deadline:
    time.sleep(0.01)
if not pathlib.Path(sys.argv[3]).exists():
    raise SystemExit("descendant did not start")
while True:
    time.sleep(1)
"""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            completed = root / "completed"
            ready = root / "ready"
            with self.assertRaises(subprocess.TimeoutExpired):
                timeout_runner.run_process(
                    [
                        sys.executable,
                        "-c",
                        parent,
                        child,
                        str(completed),
                        str(ready),
                    ],
                    timeout_seconds=0.1,
                    kill_grace_seconds=0.5,
                )

            self.assertEqual(completed.read_text(encoding="utf-8"), "completed")

    @unittest.skipUnless(timeout_runner.os.name == "posix", "POSIX process groups only")
    def test_timeout_does_not_wait_for_escaped_descendant_output_pipes(self) -> None:
        escaped_child = """
import os
import pathlib
import sys
import time

pathlib.Path(sys.argv[1]).write_text(str(os.getpid()), encoding="utf-8")
time.sleep(3)
"""
        parent = """
import pathlib
import signal
import subprocess
import sys
import time

signal.signal(signal.SIGTERM, signal.SIG_IGN)
subprocess.Popen(
    [sys.executable, "-c", sys.argv[1], sys.argv[2]],
    start_new_session=True,
)
deadline = time.monotonic() + 5
while not pathlib.Path(sys.argv[2]).exists() and time.monotonic() < deadline:
    time.sleep(0.01)
if not pathlib.Path(sys.argv[2]).exists():
    raise SystemExit("escaped descendant did not start")
while True:
    time.sleep(1)
"""
        with tempfile.TemporaryDirectory() as directory:
            pid_path = Path(directory) / "escaped.pid"
            started = time.monotonic()
            try:
                with self.assertRaises(subprocess.TimeoutExpired):
                    timeout_runner.run_process(
                        [
                            sys.executable,
                            "-c",
                            parent,
                            escaped_child,
                            str(pid_path),
                        ],
                        timeout_seconds=0.1,
                        kill_grace_seconds=0.1,
                        capture_output=True,
                        text=True,
                    )
                elapsed = time.monotonic() - started
                self.assertLess(elapsed, 1.5)
            finally:
                if pid_path.is_file():
                    try:
                        os.kill(int(pid_path.read_text(encoding="utf-8")), signal.SIGKILL)
                    except ProcessLookupError:
                        pass

    def test_rejects_invalid_parameters_and_preserves_cli_separator(self) -> None:
        with self.assertRaisesRegex(ValueError, "command"):
            timeout_runner.run_process([], timeout_seconds=1)
        with self.assertRaisesRegex(ValueError, "timeout_seconds"):
            timeout_runner.run_process([sys.executable], timeout_seconds=0)
        with self.assertRaisesRegex(ValueError, "kill_grace_seconds"):
            timeout_runner.run_process(
                [sys.executable], timeout_seconds=1, kill_grace_seconds=0
            )

        for invalid in (float("nan"), float("inf"), float("-inf")):
            with self.subTest(timeout_seconds=invalid), self.assertRaisesRegex(
                ValueError, "timeout_seconds"
            ):
                timeout_runner.run_process(
                    [sys.executable], timeout_seconds=invalid
                )
            with self.subTest(kill_grace_seconds=invalid), self.assertRaisesRegex(
                ValueError, "kill_grace_seconds"
            ):
                timeout_runner.run_process(
                    [sys.executable],
                    timeout_seconds=1,
                    kill_grace_seconds=invalid,
                )

        parsed = timeout_runner.parse_args(
            ["--timeout-seconds", "1", "--", sys.executable, "-V"]
        )
        self.assertEqual(parsed.command, [sys.executable, "-V"])

        with (
            contextlib.redirect_stderr(io.StringIO()),
            self.assertRaises(SystemExit),
        ):
            timeout_runner.parse_args(["--timeout-seconds", "1"])


if __name__ == "__main__":
    unittest.main()
