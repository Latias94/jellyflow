#!/usr/bin/env python3
"""Run the exact Open GPUI real-window smoke matrix with auditable receipts."""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import shlex
import subprocess
import sys
import tempfile
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import NamedTuple


FINAL_MARKER = "JELLYFLOW_REAL_WINDOW_SMOKE"
FINAL_MARKER_PREFIX = f"{FINAL_MARKER} "
RESULTS_NAME = "results.json"
RESULTS_SCHEMA_VERSION = 1
TIMEOUT_EXIT_CODE = 124
MARKER_KEY_PATTERN = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")


class RunSpec(NamedTuple):
    run_id: str
    demo: str
    bounds: str
    scenario: str
    zoom: str

    def config(self) -> dict[str, str]:
        return {
            "demo": self.demo,
            "bounds": self.bounds,
            "scenario": self.scenario,
            "zoom": self.zoom,
        }

    def command_arguments(self) -> list[str]:
        arguments = ["--smoke"]
        if self.demo == "workflow":
            arguments.insert(0, "--workflow")
        return arguments


RUN_SPECS = (
    RunSpec("interaction-shader-compact-z0.5", "shader", "compact", "interaction", "0.5"),
    RunSpec(
        "interaction-workflow-compact-z0.5",
        "workflow",
        "compact",
        "interaction",
        "0.5",
    ),
    RunSpec("interaction-shader-compact-z1", "shader", "compact", "interaction", "1"),
    RunSpec(
        "interaction-workflow-compact-z1",
        "workflow",
        "compact",
        "interaction",
        "1",
    ),
    RunSpec("interaction-shader-compact-z2", "shader", "compact", "interaction", "2"),
    RunSpec(
        "interaction-workflow-compact-z2",
        "workflow",
        "compact",
        "interaction",
        "2",
    ),
    RunSpec("interaction-shader-normal-z1", "shader", "normal", "interaction", "1"),
    RunSpec(
        "interaction-workflow-normal-z1",
        "workflow",
        "normal",
        "interaction",
        "1",
    ),
    RunSpec("lod-threshold-shader-normal-z1", "shader", "normal", "lod-threshold", "1"),
)


def extract_final_marker_lines(stdout: str, stderr: str) -> list[str]:
    """Return only final marker lines, excluding similarly prefixed diagnostics."""

    return [
        line
        for stream in (stdout, stderr)
        for line in stream.splitlines()
        if line.startswith(FINAL_MARKER_PREFIX)
    ]


def parse_marker_line(line: str) -> dict[str, str]:
    if not line.startswith(FINAL_MARKER_PREFIX):
        raise ValueError(f"line does not start with {FINAL_MARKER_PREFIX!r}")
    try:
        tokens = shlex.split(line, posix=True)
    except ValueError as error:
        raise ValueError(f"marker is not shell-tokenizable: {error}") from error
    if not tokens or tokens[0] != FINAL_MARKER:
        raise ValueError("marker name is malformed")

    fields: dict[str, str] = {}
    for token in tokens[1:]:
        key, separator, value = token.partition("=")
        if not separator or not MARKER_KEY_PATTERN.fullmatch(key):
            raise ValueError(f"marker token is not key=value: {token!r}")
        if key in fields:
            raise ValueError(f"marker field is duplicated: {key}")
        fields[key] = value
    return fields


def marker_contract_errors(
    spec: RunSpec,
    *,
    exit_code: int | None,
    timed_out: bool,
    marker_lines: Sequence[str],
) -> list[str]:
    errors: list[str] = []
    if timed_out:
        errors.append("run timed out")
    if exit_code != 0:
        errors.append(f"exit code is {exit_code!r}, expected 0")
    if len(marker_lines) != 1:
        errors.append(f"final marker count is {len(marker_lines)}, expected exactly 1")
        return errors

    try:
        fields = parse_marker_line(marker_lines[0])
    except ValueError as error:
        errors.append(str(error))
        return errors

    expected_fields = {
        "status": "pass",
        "demo": spec.demo,
        "bounds": spec.bounds,
        "scenario": spec.scenario,
        "target_zoom": spec.zoom,
    }
    for key, expected in expected_fields.items():
        actual = fields.get(key)
        if actual != expected:
            errors.append(f"marker {key} is {actual!r}, expected {expected!r}")
    return errors


def _text(output: str | bytes | None) -> str:
    if output is None:
        return ""
    if isinstance(output, bytes):
        return output.decode("utf-8", errors="replace")
    return output


def _relay_output(run_id: str, label: str, output: str, stream: object) -> None:
    for line in output.splitlines():
        print(f"[{run_id} {label}] {line}", file=stream, flush=True)


def execute_run(
    spec: RunSpec,
    *,
    binary: Path,
    binary_arguments: Sequence[str],
    capture_dir: Path,
    timeout_seconds: float,
    base_environment: Mapping[str, str] | None = None,
) -> dict[str, object]:
    environment = dict(os.environ if base_environment is None else base_environment)
    environment.update(
        {
            "JELLYFLOW_SMOKE_BOUNDS": spec.bounds,
            "JELLYFLOW_SMOKE_SCENARIO": spec.scenario,
            "JELLYFLOW_SMOKE_ZOOM": spec.zoom,
            "JELLYFLOW_SMOKE_CAPTURE_DIR": str(capture_dir),
        }
    )
    command = [
        str(binary),
        *binary_arguments,
        *spec.command_arguments(),
    ]
    stdout = ""
    stderr = ""
    exit_code: int | None = None
    timed_out = False
    spawn_error: str | None = None

    try:
        completed = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            env=environment,
            timeout=timeout_seconds,
        )
        stdout = completed.stdout
        stderr = completed.stderr
        exit_code = (
            completed.returncode
            if completed.returncode >= 0
            else 128 + abs(completed.returncode)
        )
    except subprocess.TimeoutExpired as error:
        stdout = _text(error.stdout)
        stderr = _text(error.stderr)
        exit_code = TIMEOUT_EXIT_CODE
        timed_out = True
    except OSError as error:
        spawn_error = f"could not start process: {error}"

    _relay_output(spec.run_id, "stdout", stdout, sys.stdout)
    _relay_output(spec.run_id, "stderr", stderr, sys.stderr)
    marker_lines = extract_final_marker_lines(stdout, stderr)
    errors = marker_contract_errors(
        spec,
        exit_code=exit_code,
        timed_out=timed_out,
        marker_lines=marker_lines,
    )
    if spawn_error is not None:
        errors.insert(0, spawn_error)

    return {
        "run_id": spec.run_id,
        "config": spec.config(),
        "exit_code": exit_code,
        "timed_out": timed_out,
        "marker_count": len(marker_lines),
        "marker": marker_lines[0] if len(marker_lines) == 1 else None,
        "stdout": stdout,
        "stderr": stderr,
        "status": "pass" if not errors else "fail",
        "errors": errors,
    }


def atomic_write_results(path: Path, runs: Sequence[dict[str, object]]) -> None:
    document = {
        "schema_version": RESULTS_SCHEMA_VERSION,
        "runs": list(runs),
    }
    contents = json.dumps(document, indent=2, sort_keys=True) + "\n"
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{path.name}.", suffix=".tmp", dir=path.parent
    )
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(contents)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def create_capture_dir(capture_root: Path) -> Path:
    capture_root.mkdir(parents=True, exist_ok=True)
    return Path(tempfile.mkdtemp(prefix="run-", dir=capture_root)).resolve()


def append_capture_dir_output(output_file: Path, capture_dir: Path) -> None:
    output_file.parent.mkdir(parents=True, exist_ok=True)
    with output_file.open("a", encoding="utf-8", newline="\n") as handle:
        handle.write(f"capture_dir={capture_dir}\n")


def run_matrix(
    *,
    binary: Path,
    binary_arguments: Sequence[str],
    capture_root: Path,
    timeout_seconds: float,
    capture_dir_output: Path | None = None,
    base_environment: Mapping[str, str] | None = None,
) -> tuple[Path, bool]:
    if not math.isfinite(timeout_seconds) or timeout_seconds <= 0:
        raise ValueError("timeout_seconds must be positive and finite")
    capture_dir = create_capture_dir(capture_root)
    if capture_dir_output is not None:
        append_capture_dir_output(capture_dir_output, capture_dir)
    print(
        f"JELLYFLOW_REAL_WINDOW_SMOKE_MATRIX capture_dir={capture_dir}",
        flush=True,
    )

    results_path = capture_dir / RESULTS_NAME
    runs: list[dict[str, object]] = []
    atomic_write_results(results_path, runs)
    for index, spec in enumerate(RUN_SPECS, start=1):
        print(
            f"running real-window smoke {index}/{len(RUN_SPECS)}: {spec.run_id}",
            flush=True,
        )
        receipt = execute_run(
            spec,
            binary=binary,
            binary_arguments=binary_arguments,
            capture_dir=capture_dir,
            timeout_seconds=timeout_seconds,
            base_environment=base_environment,
        )
        runs.append(receipt)
        atomic_write_results(results_path, runs)
        if receipt["status"] != "pass":
            print(
                f"real-window smoke run failed: {spec.run_id}: {receipt['errors']}",
                file=sys.stderr,
                flush=True,
            )

    passed = all(run["status"] == "pass" for run in runs)
    print(f"real-window smoke capture directory: {capture_dir}", flush=True)
    print(f"real-window smoke results: {results_path}", flush=True)
    return capture_dir, passed


def parse_args(arguments: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary",
        type=Path,
        default=Path(
            "examples/canvas-jellyflow/target/debug/open-gpui-canvas-jellyflow"
        ),
    )
    parser.add_argument(
        "--binary-arg",
        action="append",
        default=[],
        help="Argument inserted before the smoke demo arguments; may be repeated.",
    )
    parser.add_argument(
        "--capture-root", type=Path, default=Path("target/real-window-smoke")
    )
    parser.add_argument("--timeout-seconds", type=float, default=60.0)
    parser.add_argument(
        "--capture-dir-output",
        type=Path,
        help="Append capture_dir=<unique child> for CI step output propagation.",
    )
    args = parser.parse_args(arguments)
    if not math.isfinite(args.timeout_seconds) or args.timeout_seconds <= 0:
        parser.error("--timeout-seconds must be positive and finite")
    return args


def main(arguments: Sequence[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if arguments is None else arguments)
    try:
        _, passed = run_matrix(
            binary=args.binary,
            binary_arguments=args.binary_arg,
            capture_root=args.capture_root,
            timeout_seconds=args.timeout_seconds,
            capture_dir_output=args.capture_dir_output,
        )
    except OSError as error:
        print(f"real-window smoke matrix setup failed: {error}", file=sys.stderr)
        return 1
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
