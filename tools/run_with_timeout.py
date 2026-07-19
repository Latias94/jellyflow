#!/usr/bin/env python3
"""Run a command with a process-external timeout and terminate its process group."""

from __future__ import annotations

import argparse
import errno
import enum
import os
import signal
import subprocess
import sys
import time
from collections.abc import Sequence
from os import PathLike


TIMEOUT_EXIT_CODE = 124
DEFAULT_KILL_GRACE_SECONDS = 5.0
PROCESS_GROUP_POLL_SECONDS = 0.01


class _ProcessGroupSignalResult(enum.Enum):
    SIGNALED = enum.auto()
    MISSING = enum.auto()
    PERMISSION_DENIED = enum.auto()


def _try_signal_posix_process_group(
    process_group_id: int, signal_number: int
) -> _ProcessGroupSignalResult:
    try:
        os.killpg(process_group_id, signal_number)
    except ProcessLookupError:
        return _ProcessGroupSignalResult.MISSING
    except PermissionError:
        return _ProcessGroupSignalResult.PERMISSION_DENIED
    return _ProcessGroupSignalResult.SIGNALED


def _wait_for_posix_process_group_exit(
    process: subprocess.Popen[bytes], cleanup_deadline: float
) -> bool:
    while True:
        process.poll()
        probe = _try_signal_posix_process_group(process.pid, 0)
        if probe is _ProcessGroupSignalResult.MISSING:
            return True

        # Only ESRCH proves exit; macOS can report EPERM for a zombie group leader.
        remaining = cleanup_deadline - time.monotonic()
        if remaining <= 0:
            return False
        time.sleep(min(PROCESS_GROUP_POLL_SECONDS, remaining))


def _remaining_cleanup_seconds(cleanup_deadline: float) -> float:
    return max(0.0, cleanup_deadline - time.monotonic())


def _terminate_posix_process_group(
    process: subprocess.Popen[bytes], cleanup_deadline: float
) -> None:
    term_result = _try_signal_posix_process_group(process.pid, signal.SIGTERM)
    if term_result is _ProcessGroupSignalResult.PERMISSION_DENIED:
        raise PermissionError(
            errno.EPERM,
            f"could not terminate process group {process.pid}",
        )
    if not _wait_for_posix_process_group_exit(process, cleanup_deadline):
        kill_result = _try_signal_posix_process_group(process.pid, signal.SIGKILL)
        if kill_result is _ProcessGroupSignalResult.PERMISSION_DENIED:
            raise PermissionError(
                errno.EPERM,
                f"could not kill process group {process.pid}",
            )
    process.wait(timeout=_remaining_cleanup_seconds(cleanup_deadline))


def _terminate_single_process(
    process: subprocess.Popen[bytes], cleanup_deadline: float
) -> None:
    if process.poll() is None:
        try:
            process.terminate()
        except ProcessLookupError:
            pass

    try:
        process.wait(timeout=_remaining_cleanup_seconds(cleanup_deadline))
        return
    except subprocess.TimeoutExpired:
        pass

    if process.poll() is None:
        try:
            process.kill()
        except ProcessLookupError:
            pass
    process.wait(timeout=_remaining_cleanup_seconds(cleanup_deadline))


def _terminate_process_group(
    process: subprocess.Popen[bytes], cleanup_deadline: float
) -> None:
    if os.name == "posix":
        _terminate_posix_process_group(process, cleanup_deadline)
    else:
        _terminate_single_process(process, cleanup_deadline)


def _close_captured_pipes(process: subprocess.Popen[bytes]) -> None:
    for pipe in (process.stdout, process.stderr):
        if pipe is not None:
            pipe.close()


def _drain_captured_output(
    process: subprocess.Popen[bytes],
    original_error: BaseException,
    cleanup_deadline: float,
) -> None:
    try:
        stdout, stderr = process.communicate(
            timeout=_remaining_cleanup_seconds(cleanup_deadline)
        )
    except BaseException as drain_error:
        _close_captured_pipes(process)
        original_error.add_note(
            f"captured-output cleanup failed: {drain_error!r}"
        )
        return

    if isinstance(original_error, subprocess.TimeoutExpired):
        original_error.stdout = stdout
        original_error.stderr = stderr


def run_process(
    command: Sequence[str],
    *,
    timeout_seconds: float,
    kill_grace_seconds: float = DEFAULT_KILL_GRACE_SECONDS,
    cwd: str | PathLike[str] | None = None,
    check: bool = False,
    capture_output: bool = False,
    text: bool = False,
) -> subprocess.CompletedProcess[str] | subprocess.CompletedProcess[bytes]:
    if not command:
        raise ValueError("command must not be empty")
    if timeout_seconds <= 0:
        raise ValueError("timeout_seconds must be positive")
    if kill_grace_seconds <= 0:
        raise ValueError("kill_grace_seconds must be positive")

    process = subprocess.Popen(
        list(command),
        cwd=cwd,
        stdout=subprocess.PIPE if capture_output else None,
        stderr=subprocess.PIPE if capture_output else None,
        text=text,
        start_new_session=os.name == "posix",
    )
    try:
        if capture_output:
            stdout, stderr = process.communicate(timeout=timeout_seconds)
        else:
            process.wait(timeout=timeout_seconds)
            stdout = None
            stderr = None
    except BaseException as operation_error:
        cleanup_deadline = time.monotonic() + kill_grace_seconds
        process_group_cleaned = False
        try:
            _terminate_process_group(process, cleanup_deadline)
            process_group_cleaned = True
        except BaseException as cleanup_error:
            try:
                _terminate_single_process(process, cleanup_deadline)
            except BaseException as fallback_error:
                cleanup_error.add_note(
                    f"direct-process cleanup also failed: {fallback_error!r}"
                )
            operation_error.add_note(
                f"process-group cleanup failed: {cleanup_error!r}"
            )
        if capture_output:
            if process_group_cleaned:
                _drain_captured_output(
                    process,
                    operation_error,
                    cleanup_deadline,
                )
            else:
                _close_captured_pipes(process)
        raise

    completed = subprocess.CompletedProcess(
        process.args,
        process.returncode,
        stdout,
        stderr,
    )
    if check:
        completed.check_returncode()
    return completed


def run_with_timeout(
    command: Sequence[str],
    timeout_seconds: float,
    kill_grace_seconds: float = DEFAULT_KILL_GRACE_SECONDS,
) -> int:
    try:
        completed = run_process(
            command,
            timeout_seconds=timeout_seconds,
            kill_grace_seconds=kill_grace_seconds,
        )
    except subprocess.TimeoutExpired as error:
        print(
            f"command timed out after {timeout_seconds:g}s: {' '.join(command)}",
            file=sys.stderr,
            flush=True,
        )
        for note in getattr(error, "__notes__", ()):
            print(note, file=sys.stderr, flush=True)
        return TIMEOUT_EXIT_CODE

    return_code = completed.returncode
    return return_code if return_code >= 0 else 128 + abs(return_code)


def parse_args(arguments: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timeout-seconds", type=float, required=True)
    parser.add_argument(
        "--kill-grace-seconds",
        type=float,
        default=DEFAULT_KILL_GRACE_SECONDS,
    )
    parser.add_argument("command", nargs=argparse.REMAINDER)
    parsed = parser.parse_args(arguments)
    if parsed.command[:1] == ["--"]:
        parsed.command = parsed.command[1:]
    if not parsed.command:
        parser.error("a command is required after --")
    return parsed


def main(arguments: Sequence[str] | None = None) -> int:
    parsed = parse_args(sys.argv[1:] if arguments is None else arguments)
    try:
        return run_with_timeout(
            parsed.command,
            parsed.timeout_seconds,
            parsed.kill_grace_seconds,
        )
    except ValueError as error:
        print(error, file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
