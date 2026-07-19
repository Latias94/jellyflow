from __future__ import annotations

import importlib.util
import signal
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, call, patch


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "run_with_timeout.py"
SPEC = importlib.util.spec_from_file_location("run_with_timeout", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {SCRIPT_PATH}")
timeout_runner = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = timeout_runner
SPEC.loader.exec_module(timeout_runner)


class RunWithTimeoutTests(unittest.TestCase):
    def test_returns_the_child_exit_code(self) -> None:
        self.assertEqual(
            timeout_runner.run_with_timeout(
                [sys.executable, "-c", "raise SystemExit(7)"],
                timeout_seconds=5,
            ),
            7,
        )

    def test_terminates_a_stalled_process_at_the_external_deadline(self) -> None:
        process = Mock()
        process.wait.side_effect = subprocess.TimeoutExpired("probe", 0.05)

        with (
            patch.object(timeout_runner.subprocess, "Popen", return_value=process),
            patch.object(timeout_runner, "_terminate_process_group") as terminate,
            patch.object(timeout_runner.time, "monotonic", return_value=10.0),
        ):
            exit_code = timeout_runner.run_with_timeout(
                [sys.executable, "-c", "pass"],
                timeout_seconds=0.05,
                kill_grace_seconds=0.05,
            )

        self.assertEqual(exit_code, timeout_runner.TIMEOUT_EXIT_CODE)
        terminate.assert_called_once_with(process, 10.05)

    def test_non_timeout_wait_exception_still_terminates_the_process_group(self) -> None:
        process = Mock()
        interrupted = KeyboardInterrupt()
        process.wait.side_effect = interrupted

        with (
            patch.object(timeout_runner.subprocess, "Popen", return_value=process),
            patch.object(timeout_runner, "_terminate_process_group") as terminate,
            patch.object(timeout_runner.time, "monotonic", return_value=10.0),
            self.assertRaises(KeyboardInterrupt) as raised,
        ):
            timeout_runner.run_process(
                [sys.executable, "-c", "pass"],
                timeout_seconds=1,
            )

        self.assertIs(raised.exception, interrupted)
        terminate.assert_called_once_with(
            process,
            10.0 + timeout_runner.DEFAULT_KILL_GRACE_SECONDS,
        )

    def test_capture_timeout_drains_pipes_after_group_cleanup(self) -> None:
        process = Mock()
        timeout = subprocess.TimeoutExpired("probe", 0.05)
        process.communicate.side_effect = [timeout, ("remaining stdout", "remaining stderr")]

        with (
            patch.object(timeout_runner.subprocess, "Popen", return_value=process),
            patch.object(timeout_runner, "_terminate_process_group") as terminate,
            patch.object(
                timeout_runner.time,
                "monotonic",
                side_effect=(10.0, 10.25),
            ),
            self.assertRaises(subprocess.TimeoutExpired) as raised,
        ):
            timeout_runner.run_process(
                [sys.executable, "-c", "pass"],
                timeout_seconds=0.05,
                capture_output=True,
                text=True,
            )

        self.assertIs(raised.exception, timeout)
        self.assertEqual(raised.exception.stdout, "remaining stdout")
        self.assertEqual(raised.exception.stderr, "remaining stderr")
        self.assertEqual(
            process.communicate.call_args_list,
            [call(timeout=0.05), call(timeout=4.75)],
        )
        terminate.assert_called_once_with(
            process,
            10.0 + timeout_runner.DEFAULT_KILL_GRACE_SECONDS,
        )

    def test_capture_cleanup_closes_pipes_when_drain_exhausts_the_deadline(self) -> None:
        process = Mock()
        timeout = subprocess.TimeoutExpired("probe", 0.05)
        drain_timeout = subprocess.TimeoutExpired("drain", 0.05)
        process.communicate.side_effect = [timeout, drain_timeout]

        with (
            patch.object(timeout_runner.subprocess, "Popen", return_value=process),
            patch.object(timeout_runner, "_terminate_process_group"),
            patch.object(
                timeout_runner.time,
                "monotonic",
                side_effect=(10.0, 10.05),
            ),
            self.assertRaises(subprocess.TimeoutExpired) as raised,
        ):
            timeout_runner.run_process(
                [sys.executable, "-c", "pass"],
                timeout_seconds=0.05,
                kill_grace_seconds=0.1,
                capture_output=True,
            )

        self.assertIs(raised.exception, timeout)
        self.assertEqual(process.communicate.call_count, 2)
        self.assertEqual(process.communicate.call_args_list[0], call(timeout=0.05))
        self.assertAlmostEqual(
            process.communicate.call_args_list[1].kwargs["timeout"],
            0.05,
        )
        process.stdout.close.assert_called_once_with()
        process.stderr.close.assert_called_once_with()
        self.assertTrue(
            any(
                "captured-output cleanup failed" in note
                for note in raised.exception.__notes__
            )
        )

    def test_cleanup_fallback_reuses_the_process_group_deadline(self) -> None:
        process = Mock()
        interrupted = KeyboardInterrupt()
        process.wait.side_effect = interrupted
        group_error = PermissionError("group cleanup denied")

        with (
            patch.object(timeout_runner.subprocess, "Popen", return_value=process),
            patch.object(
                timeout_runner,
                "_terminate_process_group",
                side_effect=group_error,
            ) as terminate_group,
            patch.object(timeout_runner, "_terminate_single_process") as terminate_single,
            patch.object(timeout_runner.time, "monotonic", return_value=20.0),
            self.assertRaises(KeyboardInterrupt) as raised,
        ):
            timeout_runner.run_process(
                [sys.executable, "-c", "pass"],
                timeout_seconds=1,
                kill_grace_seconds=0.25,
            )

        self.assertIs(raised.exception, interrupted)
        terminate_group.assert_called_once_with(process, 20.25)
        terminate_single.assert_called_once_with(process, 20.25)

    def test_direct_process_kill_and_reap_share_one_deadline(self) -> None:
        process = Mock()
        process.poll.side_effect = (None, None)
        first_timeout = subprocess.TimeoutExpired("term", 0.3)
        second_timeout = subprocess.TimeoutExpired("kill", 0.01)
        process.wait.side_effect = (first_timeout, second_timeout)

        with (
            patch.object(
                timeout_runner.time,
                "monotonic",
                side_effect=(0.2, 0.49),
            ),
            self.assertRaises(subprocess.TimeoutExpired) as raised,
        ):
            timeout_runner._terminate_single_process(process, cleanup_deadline=0.5)

        self.assertIs(raised.exception, second_timeout)
        process.terminate.assert_called_once_with()
        process.kill.assert_called_once_with()
        self.assertEqual(process.wait.call_count, 2)
        self.assertEqual(process.wait.call_args_list[0], call(timeout=0.3))
        self.assertAlmostEqual(
            process.wait.call_args_list[1].kwargs["timeout"],
            0.01,
        )

    @unittest.skipUnless(timeout_runner.os.name == "posix", "POSIX process groups only")
    def test_kills_a_descendant_that_outlives_the_group_leader(self) -> None:
        process = Mock(pid=4100)
        process.poll.return_value = 0
        process.wait.return_value = 0

        with (
            patch.object(timeout_runner.os, "killpg") as killpg,
            patch.object(timeout_runner.time, "monotonic", side_effect=(1.0, 1.0)),
        ):
            timeout_runner._terminate_process_group(process, cleanup_deadline=0.5)

        self.assertEqual(
            killpg.call_args_list,
            [call(4100, signal.SIGTERM), call(4100, 0), call(4100, signal.SIGKILL)],
        )
        process.wait.assert_called_once_with(timeout=0.0)

    @unittest.skipUnless(timeout_runner.os.name == "posix", "POSIX process groups only")
    def test_does_not_kill_when_descendants_exit_before_the_leader(self) -> None:
        process = Mock(pid=4200)
        process.poll.side_effect = (None, 0)
        process.wait.return_value = 0

        with (
            patch.object(
                timeout_runner.os,
                "killpg",
                side_effect=(None, None, ProcessLookupError()),
            ) as killpg,
            patch.object(timeout_runner.time, "monotonic", side_effect=(0.1, 0.2)),
            patch.object(timeout_runner.time, "sleep") as sleep,
        ):
            timeout_runner._terminate_process_group(process, cleanup_deadline=0.5)

        self.assertEqual(
            killpg.call_args_list,
            [call(4200, signal.SIGTERM), call(4200, 0), call(4200, 0)],
        )
        sleep.assert_called_once_with(timeout_runner.PROCESS_GROUP_POLL_SECONDS)
        process.wait.assert_called_once_with(timeout=0.3)

    @unittest.skipUnless(timeout_runner.os.name == "posix", "POSIX process groups only")
    def test_group_disappearing_before_a_signal_still_reaps_the_leader(self) -> None:
        process = Mock(pid=4300)
        process.poll.return_value = 0
        process.wait.return_value = 0

        with (
            patch.object(
                timeout_runner.os,
                "killpg",
                side_effect=(ProcessLookupError(), ProcessLookupError()),
            ) as killpg,
            patch.object(timeout_runner.time, "monotonic", return_value=0.1),
        ):
            timeout_runner._terminate_process_group(process, cleanup_deadline=0.5)

        self.assertEqual(
            killpg.call_args_list,
            [call(4300, signal.SIGTERM), call(4300, 0)],
        )
        process.wait.assert_called_once_with(timeout=0.4)

    def test_rejects_an_empty_command_and_non_positive_deadlines(self) -> None:
        with self.assertRaisesRegex(ValueError, "command"):
            timeout_runner.run_with_timeout([], timeout_seconds=1)
        with self.assertRaisesRegex(ValueError, "timeout_seconds"):
            timeout_runner.run_with_timeout([sys.executable], timeout_seconds=0)
        with self.assertRaisesRegex(ValueError, "kill_grace_seconds"):
            timeout_runner.run_with_timeout(
                [sys.executable], timeout_seconds=1, kill_grace_seconds=0
            )


if __name__ == "__main__":
    unittest.main()
