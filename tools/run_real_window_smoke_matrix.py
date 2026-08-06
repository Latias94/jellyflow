#!/usr/bin/env python3
"""Run the exact Open GPUI real-window matrix and transport minimal receipts."""

from __future__ import annotations

import argparse
import json
import math
import os
import subprocess
import sys
import tempfile
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import NamedTuple

try:
    from run_with_timeout import TIMEOUT_EXIT_CODE, run_process
except ModuleNotFoundError:
    from tools.run_with_timeout import TIMEOUT_EXIT_CODE, run_process


RECEIPT_PREFIX = "JELLYFLOW_REAL_WINDOW_SMOKE_RECEIPT "
RECEIPT_SCHEMA_VERSION = 1
RECEIPT_FIELDS = frozenset(
    {"schema_version", "run_id", "status", "failure", "artifacts"}
)
RESULTS_NAME = "results.json"
RESULTS_SCHEMA_VERSION = 1


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
        arguments: list[str] = []
        if self.demo == "workflow":
            arguments.append("--workflow")
        return arguments


RUN_SPECS = (
    RunSpec(
        "interaction-shader-compact-z0.5",
        "shader",
        "compact",
        "interaction",
        "0.5",
    ),
    RunSpec(
        "interaction-workflow-compact-z1",
        "workflow",
        "compact",
        "interaction",
        "1",
    ),
    RunSpec("interaction-shader-compact-z2", "shader", "compact", "interaction", "2"),
    RunSpec("interaction-shader-normal-z1", "shader", "normal", "interaction", "1"),
    RunSpec("lod-threshold-shader-normal-z1", "shader", "normal", "lod-threshold", "1"),
)


class ReceiptTransportError(RuntimeError):
    """A native receipt could not be transported into the matrix results."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code


def extract_receipt_payloads(stdout: str, stderr: str) -> list[str]:
    """Extract payloads from exact receipt lines in both captured streams."""

    return [
        line.removeprefix(RECEIPT_PREFIX)
        for stream in (stdout, stderr)
        for line in stream.splitlines()
        if line.startswith(RECEIPT_PREFIX)
    ]


def reject_duplicate_json_object_keys(
    pairs: list[tuple[str, object]],
) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"JSON object contains duplicate key {key!r}")
        result[key] = value
    return result


def _parse_json_object(payload: str) -> dict[str, object]:
    try:
        value = json.loads(
            payload,
            object_pairs_hook=reject_duplicate_json_object_keys,
        )
    except (json.JSONDecodeError, ValueError) as error:
        raise ReceiptTransportError(
            "malformed_receipt", f"receipt is not unambiguous JSON: {error}"
        ) from error
    if not isinstance(value, dict):
        raise ReceiptTransportError(
            "malformed_receipt", "receipt root must be a JSON object"
        )
    return value


def validate_receipt(
    payload: str, spec: RunSpec, capture_dir: Path
) -> dict[str, object]:
    """Validate only the versioned process-transport contract."""

    receipt = _parse_json_object(payload)
    if receipt.keys() != RECEIPT_FIELDS:
        raise ReceiptTransportError(
            "malformed_receipt",
            f"receipt fields are {sorted(receipt)}, expected {sorted(RECEIPT_FIELDS)}",
        )

    version = receipt["schema_version"]
    if type(version) is not int or version != RECEIPT_SCHEMA_VERSION:
        raise ReceiptTransportError(
            "unsupported_receipt_version",
            f"schema_version is {version!r}, expected {RECEIPT_SCHEMA_VERSION}",
        )

    run_id = receipt["run_id"]
    if not isinstance(run_id, str):
        raise ReceiptTransportError("malformed_receipt", "run_id must be a string")
    if run_id != spec.run_id:
        raise ReceiptTransportError(
            "receipt_run_mismatch",
            f"receipt run_id is {run_id!r}, expected {spec.run_id!r}",
        )

    status = receipt["status"]
    if not isinstance(status, str) or status not in {"pass", "fail"}:
        raise ReceiptTransportError(
            "malformed_receipt", "status must be 'pass' or 'fail'"
        )
    failure = receipt["failure"]
    if status == "pass" and failure is not None:
        raise ReceiptTransportError(
            "malformed_receipt", "pass receipt must not include a failure"
        )
    if status == "fail":
        if not isinstance(failure, dict) or not isinstance(failure.get("code"), str):
            raise ReceiptTransportError(
                "malformed_receipt", "fail receipt must include a failure code"
            )
        message = failure.get("message")
        if message is not None and not isinstance(message, str):
            raise ReceiptTransportError(
                "malformed_receipt", "failure message must be a string or null"
            )

    artifacts = receipt["artifacts"]
    if not isinstance(artifacts, list) or not all(
        isinstance(artifact, str) and artifact for artifact in artifacts
    ):
        raise ReceiptTransportError(
            "malformed_receipt", "artifacts must be a list of non-empty paths"
        )
    if status == "pass" and not artifacts:
        raise ReceiptTransportError(
            "missing_artifact", "pass receipt lists no review artifacts"
        )
    artifact_paths = [Path(artifact) for artifact in artifacts]
    missing = [artifact for artifact in artifact_paths if not artifact.is_file()]
    if missing:
        raise ReceiptTransportError(
            "missing_artifact",
            f"listed artifacts do not exist or are not files: {missing!r}",
        )
    capture_dir = capture_dir.resolve()
    resolved_artifacts = [artifact.resolve() for artifact in artifact_paths]
    outside_capture = [
        artifact
        for artifact in resolved_artifacts
        if not artifact.is_relative_to(capture_dir)
    ]
    if outside_capture:
        raise ReceiptTransportError(
            "artifact_outside_capture",
            f"listed artifacts are outside this run's capture directory: {outside_capture!r}",
        )
    if status == "pass":
        non_png = [
            artifact
            for artifact in resolved_artifacts
            if artifact.suffix.lower() != ".png"
        ]
        if non_png:
            raise ReceiptTransportError(
                "invalid_artifact",
                f"pass receipt artifacts must be PNG files: {non_png!r}",
            )
    if len(set(resolved_artifacts)) != len(resolved_artifacts):
        raise ReceiptTransportError(
            "duplicate_artifact", "receipt lists the same artifact more than once"
        )
    return receipt


def _text(output: str | bytes | None) -> str:
    if output is None:
        return ""
    if isinstance(output, bytes):
        return output.decode("utf-8", errors="replace")
    return output


def _relay_output(run_id: str, label: str, output: str, stream: object) -> None:
    for line in output.splitlines():
        print(f"[{run_id} {label}] {line}", file=stream, flush=True)


def _failure(code: str, message: str) -> str:
    return f"transport.{code}: {message}"


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
    command = [str(binary), *binary_arguments, *spec.command_arguments()]
    stdout = ""
    stderr = ""
    exit_code: int | None = None
    timed_out = False
    errors: list[str] = []

    try:
        completed = run_process(
            command,
            timeout_seconds=timeout_seconds,
            env=environment,
            capture_output=True,
            text=True,
        )
        stdout = _text(completed.stdout)
        stderr = _text(completed.stderr)
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
        errors.append(
            _failure("timeout", f"run exceeded {timeout_seconds:g} seconds")
        )
    except OSError as error:
        errors.append(_failure("spawn_failure", f"could not start process: {error}"))

    _relay_output(spec.run_id, "stdout", stdout, sys.stdout)
    _relay_output(spec.run_id, "stderr", stderr, sys.stderr)
    payloads = extract_receipt_payloads(stdout, stderr)
    receipt: dict[str, object] | None = None

    if not timed_out and not errors:
        if not payloads:
            errors.append(_failure("missing_receipt", "native run emitted no receipt"))
        elif len(payloads) > 1:
            errors.append(
                _failure(
                    "duplicate_receipt",
                    f"native run emitted {len(payloads)} receipts, expected exactly one",
                )
            )
        else:
            try:
                receipt = validate_receipt(payloads[0], spec, capture_dir)
            except ReceiptTransportError as error:
                errors.append(_failure(error.code, str(error)))

    if receipt is not None:
        receipt_status = receipt["status"]
        if (exit_code == 0 and receipt_status != "pass") or (
            exit_code not in (None, 0) and receipt_status == "pass"
        ):
            errors.append(
                _failure(
                    "exit_status_contradiction",
                    f"exit code {exit_code!r} contradicts receipt status {receipt_status!r}",
                )
            )
        elif exit_code != 0:
            errors.append(_failure("nonzero_exit", f"native run exited with {exit_code!r}"))
    elif exit_code not in (None, 0, TIMEOUT_EXIT_CODE) and not timed_out:
        errors.append(_failure("nonzero_exit", f"native run exited with {exit_code!r}"))

    passed = not errors
    return {
        "run_id": spec.run_id,
        "config": spec.config(),
        "exit_code": exit_code,
        "timed_out": timed_out,
        "receipt_count": len(payloads),
        "receipt": receipt
        if passed or (receipt is not None and receipt["status"] == "fail")
        else None,
        "stdout": stdout,
        "stderr": stderr,
        "status": "pass" if passed else "fail",
        "errors": errors,
    }


def atomic_write_results(path: Path, runs: Sequence[dict[str, object]]) -> None:
    document = {"schema_version": RESULTS_SCHEMA_VERSION, "runs": list(runs)}
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
    print(f"JELLYFLOW_REAL_WINDOW_SMOKE_MATRIX capture_dir={capture_dir}", flush=True)

    results_path = capture_dir / RESULTS_NAME
    runs: list[dict[str, object]] = []
    claimed_artifacts: set[Path] = set()
    atomic_write_results(results_path, runs)
    for index, spec in enumerate(RUN_SPECS, start=1):
        print(
            f"running real-window smoke {index}/{len(RUN_SPECS)}: {spec.run_id}",
            flush=True,
        )
        run = execute_run(
            spec,
            binary=binary,
            binary_arguments=binary_arguments,
            capture_dir=capture_dir,
            timeout_seconds=timeout_seconds,
            base_environment=base_environment,
        )
        receipt = run["receipt"]
        if isinstance(receipt, dict):
            artifacts = {
                Path(artifact).resolve()
                for artifact in receipt["artifacts"]
                if isinstance(artifact, str)
            }
            reused = sorted(artifacts & claimed_artifacts)
            if reused:
                run["errors"].append(
                    _failure(
                        "duplicate_artifact",
                        f"artifacts were already claimed by an earlier run: {reused!r}",
                    )
                )
                run["receipt"] = None
                run["status"] = "fail"
            else:
                claimed_artifacts.update(artifacts)
        runs.append(run)
        atomic_write_results(results_path, runs)
        if run["status"] != "pass":
            print(
                f"real-window smoke run failed: {spec.run_id}: {run['errors']}",
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
            "examples/canvas-jellyflow/target/debug/open-gpui-real-window-smoke"
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
