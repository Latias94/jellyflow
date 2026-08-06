#!/usr/bin/env python3
"""Run a command with a timeout and terminate its process tree on expiry."""

from __future__ import annotations

import argparse
import math
import os
import signal
import subprocess
import sys
import time
from collections.abc import Mapping, Sequence
from os import PathLike


TIMEOUT_EXIT_CODE = 124
DEFAULT_KILL_GRACE_SECONDS = 5.0
PROCESS_GROUP_POLL_SECONDS = 0.01


def _remaining_seconds(deadline: float) -> float:
    return max(0.0, deadline - time.monotonic())


def _posix_process_group_exists(process_group: int) -> bool:
    try:
        os.killpg(process_group, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _wait_for_posix_process_group(process: subprocess.Popen[bytes], deadline: float) -> bool:
    while True:
        process.poll()
        if not _posix_process_group_exists(process.pid):
            return True
        remaining = _remaining_seconds(deadline)
        if remaining <= 0:
            return False
        time.sleep(min(PROCESS_GROUP_POLL_SECONDS, remaining))


def _taskkill_windows_process_tree(
    process: subprocess.Popen[bytes], *, force: bool, timeout_seconds: float
) -> None:
    if timeout_seconds <= 0:
        return
    command = ["taskkill", "/PID", str(process.pid), "/T"]
    if force:
        command.append("/F")
    try:
        subprocess.run(
            command,
            check=False,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=timeout_seconds,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass


def _terminate_process_group(
    process: subprocess.Popen[bytes], kill_grace_seconds: float
) -> float:
    started = time.monotonic()
    deadline = started + kill_grace_seconds
    termination_deadline = started + kill_grace_seconds * 0.8
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        if not _wait_for_posix_process_group(process, termination_deadline):
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
    else:
        _taskkill_windows_process_tree(
            process,
            force=False,
            timeout_seconds=_remaining_seconds(termination_deadline),
        )
        remaining = _remaining_seconds(termination_deadline)
        if remaining > 0:
            try:
                process.wait(timeout=remaining)
            except subprocess.TimeoutExpired:
                pass
        if process.poll() is None:
            _taskkill_windows_process_tree(
                process,
                force=True,
                timeout_seconds=_remaining_seconds(deadline),
            )
        if process.poll() is None:
            process.kill()

    remaining = _remaining_seconds(deadline)
    if remaining > 0:
        try:
            process.wait(timeout=remaining)
        except subprocess.TimeoutExpired:
            pass
    else:
        process.poll()
    return deadline


def run_process(
    command: Sequence[str],
    *,
    timeout_seconds: float,
    kill_grace_seconds: float = DEFAULT_KILL_GRACE_SECONDS,
    cwd: str | PathLike[str] | None = None,
    env: Mapping[str, str] | None = None,
    input: str | bytes | None = None,
    check: bool = False,
    capture_output: bool = False,
    text: bool = False,
) -> subprocess.CompletedProcess[str] | subprocess.CompletedProcess[bytes]:
    if not command:
        raise ValueError("command must not be empty")
    if not math.isfinite(timeout_seconds) or timeout_seconds <= 0:
        raise ValueError("timeout_seconds must be positive")
    if not math.isfinite(kill_grace_seconds) or kill_grace_seconds <= 0:
        raise ValueError("kill_grace_seconds must be positive")

    process = subprocess.Popen(
        list(command),
        cwd=cwd,
        env=env,
        stdin=subprocess.PIPE if input is not None else None,
        stdout=subprocess.PIPE if capture_output else None,
        stderr=subprocess.PIPE if capture_output else None,
        text=text,
        start_new_session=os.name == "posix",
        creationflags=(
            getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
            if os.name == "nt"
            else 0
        ),
    )
    try:
        if capture_output or input is not None:
            stdout, stderr = process.communicate(
                input=input,
                timeout=timeout_seconds,
            )
        else:
            process.wait(timeout=timeout_seconds)
            stdout = None
            stderr = None
    except BaseException as error:
        cleanup_deadline = _terminate_process_group(process, kill_grace_seconds)
        if capture_output or input is not None:
            try:
                cleanup_stdout, cleanup_stderr = process.communicate(
                    timeout=_remaining_seconds(cleanup_deadline)
                )
            except subprocess.TimeoutExpired as cleanup_error:
                cleanup_stdout = cleanup_error.stdout
                cleanup_stderr = cleanup_error.stderr
                for pipe in (process.stdin, process.stdout, process.stderr):
                    if pipe is not None:
                        try:
                            pipe.close()
                        except OSError:
                            pass
            if isinstance(error, subprocess.TimeoutExpired):
                if cleanup_stdout is not None:
                    error.stdout = cleanup_stdout
                if cleanup_stderr is not None:
                    error.stderr = cleanup_stderr
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
    except subprocess.TimeoutExpired:
        print(
            f"command timed out after {timeout_seconds:g}s: {' '.join(command)}",
            file=sys.stderr,
            flush=True,
        )
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
