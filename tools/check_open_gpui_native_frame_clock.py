#!/usr/bin/env python3
"""Run and validate the non-test-support Open GPUI production frame-clock witness."""

from __future__ import annotations

import argparse
import dataclasses
import math
import platform
import subprocess
import sys
from pathlib import Path


TOOLS_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_ROOT))
try:
    from run_with_timeout import TIMEOUT_EXIT_CODE, run_process
finally:
    sys.path.pop(0)


ROOT = TOOLS_ROOT.parent
DEFAULT_BINARY = (
    ROOT
    / "examples"
    / "canvas-jellyflow"
    / "target"
    / "debug"
    / "open-gpui-native-frame-clock-witness"
)
MARKER_PREFIX = "JELLYFLOW_NATIVE_FRAME_CLOCK_WITNESS "
DEFAULT_TIMEOUT_SECONDS = 30.0
REQUIRED_POST_RELEASE_FRAMES = 5
MAX_POST_RELEASE_SETTLE_FRAMES = 16
MAX_POST_RELEASE_REDRAW_REQUESTS = 256
PAN_EPSILON = 0.001
EXPECTED_MARKER_FIELDS = {
    "status",
    "test_support",
    "native_frame_request",
    "post_release_stable",
    "post_release_settle_frames",
    "redraw_request_count",
    "redraw_driver_active",
    "post_release_frames",
    "post_release_sequence",
    "callback_entries",
    "callback_registrations",
    "frame_requests",
    "pan_samples",
    "pan_delta_x",
}


@dataclasses.dataclass(frozen=True)
class PostReleaseFrame:
    index: int
    phase: str
    pan_x: float
    pan_y: float
    zoom: float
    callback_entries: int
    callback_registrations: int
    frame_requests: int
    callback_epoch: int
    pending_queries: int
    pending_submission: bool
    ready_prefixes: int
    tick_scheduled: bool
    tick_frame_requested: bool
    query_frame_scheduled: bool
    query_resolution_active: bool
    native_tick_wake_active: bool
    native_query_wake_active: bool


@dataclasses.dataclass(frozen=True)
class FrameClockEvidence:
    callback_entries: int
    callback_registrations: int
    frame_requests: int
    pan_samples: int
    pan_delta_x: float
    settle_frames: int
    redraw_requests: int
    post_release_frames: tuple[PostReleaseFrame, ...]


def _parse_nonnegative_int(value: str, label: str) -> int:
    try:
        parsed = int(value)
    except ValueError as error:
        raise RuntimeError(f"{label} is not an integer: {value!r}") from error
    if parsed < 0:
        raise RuntimeError(f"{label} is negative: {parsed}")
    return parsed


def _parse_finite_float(value: str, label: str) -> float:
    try:
        parsed = float(value)
    except ValueError as error:
        raise RuntimeError(f"{label} is not a number: {value!r}") from error
    if not math.isfinite(parsed):
        raise RuntimeError(f"{label} is not finite: {value!r}")
    return parsed


def _parse_binary_flag(value: str, label: str) -> bool:
    if value not in {"0", "1"}:
        raise RuntimeError(f"{label} is not a 0/1 flag: {value!r}")
    return value == "1"


def _parse_post_release_frame(encoded: str, expected_index: int) -> PostReleaseFrame:
    parts = encoded.split(":")
    if len(parts) != 18:
        raise RuntimeError(
            f"post-release frame {expected_index} has {len(parts)} fields, expected 18"
        )
    index = _parse_nonnegative_int(parts[0], "post-release frame index")
    if index != expected_index:
        raise RuntimeError(
            f"post-release frame index is {index}, expected {expected_index}"
        )
    return PostReleaseFrame(
        index=index,
        phase=parts[1],
        pan_x=_parse_finite_float(parts[2], f"frame {index} pan_x"),
        pan_y=_parse_finite_float(parts[3], f"frame {index} pan_y"),
        zoom=_parse_finite_float(parts[4], f"frame {index} zoom"),
        callback_entries=_parse_nonnegative_int(parts[5], f"frame {index} callback_entries"),
        callback_registrations=_parse_nonnegative_int(
            parts[6], f"frame {index} callback_registrations"
        ),
        frame_requests=_parse_nonnegative_int(parts[7], f"frame {index} frame_requests"),
        callback_epoch=_parse_nonnegative_int(parts[8], f"frame {index} callback_epoch"),
        pending_queries=_parse_nonnegative_int(parts[9], f"frame {index} pending_queries"),
        pending_submission=_parse_binary_flag(parts[10], f"frame {index} pending_submission"),
        ready_prefixes=_parse_nonnegative_int(parts[11], f"frame {index} ready_prefixes"),
        tick_scheduled=_parse_binary_flag(parts[12], f"frame {index} tick_scheduled"),
        tick_frame_requested=_parse_binary_flag(
            parts[13], f"frame {index} tick_frame_requested"
        ),
        query_frame_scheduled=_parse_binary_flag(
            parts[14], f"frame {index} query_frame_scheduled"
        ),
        query_resolution_active=_parse_binary_flag(
            parts[15], f"frame {index} query_resolution_active"
        ),
        native_tick_wake_active=_parse_binary_flag(
            parts[16], f"frame {index} native_tick_wake_active"
        ),
        native_query_wake_active=_parse_binary_flag(
            parts[17], f"frame {index} native_query_wake_active"
        ),
    )


def _validate_post_release_sequence(frames: tuple[PostReleaseFrame, ...]) -> None:
    if len(frames) != REQUIRED_POST_RELEASE_FRAMES:
        raise RuntimeError(
            f"post-release sequence has {len(frames)} frames, expected {REQUIRED_POST_RELEASE_FRAMES}"
        )
    for frame in frames:
        if frame.phase != "Idle":
            raise RuntimeError(f"post-release frame {frame.index} phase is {frame.phase!r}")
        pending = [
            label
            for present, label in (
                (frame.pending_queries != 0, "queries"),
                (frame.pending_submission, "submission"),
                (frame.ready_prefixes != 0, "ready-prefixes"),
                (frame.tick_scheduled, "tick-token"),
                (frame.tick_frame_requested, "tick-frame-request"),
                (frame.query_frame_scheduled, "query-token"),
                (frame.query_resolution_active, "query-resolution"),
                (frame.native_tick_wake_active, "native-tick-wake"),
                (frame.native_query_wake_active, "native-query-wake"),
            )
            if present
        ]
        if pending:
            raise RuntimeError(
                f"post-release frame {frame.index} retains pending {','.join(pending)}"
            )
        if frame.callback_entries > frame.callback_registrations:
            raise RuntimeError(
                f"post-release frame {frame.index} callback entries exceed registrations"
            )

    baseline = frames[0]
    for frame in frames[1:]:
        if frame.phase != baseline.phase:
            raise RuntimeError("post-release interaction phase changed across native frames")
        if any(
            abs(actual - expected) > PAN_EPSILON
            for actual, expected in (
                (frame.pan_x, baseline.pan_x),
                (frame.pan_y, baseline.pan_y),
                (frame.zoom, baseline.zoom),
            )
        ):
            raise RuntimeError("post-release viewport changed across native frames")
        if frame.callback_entries != baseline.callback_entries:
            raise RuntimeError("post-release callback entries changed across native frames")
        if frame.callback_registrations != baseline.callback_registrations:
            raise RuntimeError("post-release callback registrations changed across native frames")
        if frame.frame_requests != baseline.frame_requests:
            raise RuntimeError("post-release frame requests changed across native frames")
        if frame.callback_epoch != baseline.callback_epoch:
            raise RuntimeError("post-release callback epoch changed across native frames")


def parse_pass_marker(output: str) -> FrameClockEvidence:
    markers = [line for line in output.splitlines() if line.startswith(MARKER_PREFIX)]
    if len(markers) != 1:
        raise RuntimeError(f"expected exactly one frame-clock marker, found {len(markers)}")
    fields: dict[str, str] = {}
    for item in markers[0][len(MARKER_PREFIX) :].split():
        key, separator, value = item.partition("=")
        if not separator or not key or not value:
            raise RuntimeError(f"malformed frame-clock marker field: {item!r}")
        if key in fields:
            raise RuntimeError(f"duplicate frame-clock marker field: {key}")
        fields[key] = value
    actual_fields = set(fields)
    if actual_fields != EXPECTED_MARKER_FIELDS:
        raise RuntimeError(
            "frame-clock marker fields mismatch; "
            f"missing={sorted(EXPECTED_MARKER_FIELDS - actual_fields)}; "
            f"unexpected={sorted(actual_fields - EXPECTED_MARKER_FIELDS)}"
        )
    if fields.get("status") != "pass":
        raise RuntimeError("frame-clock marker did not report status=pass")
    if fields.get("test_support") != "false":
        raise RuntimeError("frame-clock witness did not prove a non-test-support build")
    if fields.get("native_frame_request") != "host-native-frame":
        raise RuntimeError("frame-clock witness did not use the host native-frame request")
    if fields.get("post_release_stable") != "true":
        raise RuntimeError("frame-clock callbacks did not stop after drag release")
    settle_frames = _parse_nonnegative_int(
        fields["post_release_settle_frames"], "post_release_settle_frames"
    )
    if not 1 <= settle_frames <= MAX_POST_RELEASE_SETTLE_FRAMES:
        raise RuntimeError(
            "post-release settle count is "
            f"{settle_frames}, expected 1..={MAX_POST_RELEASE_SETTLE_FRAMES}"
        )
    redraw_requests = _parse_nonnegative_int(
        fields["redraw_request_count"], "redraw_request_count"
    )
    if not 1 <= redraw_requests <= MAX_POST_RELEASE_REDRAW_REQUESTS:
        raise RuntimeError(
            "post-release redraw request count is "
            f"{redraw_requests}, expected 1..={MAX_POST_RELEASE_REDRAW_REQUESTS}"
        )
    if fields["redraw_driver_active"] != "false":
        raise RuntimeError("post-release redraw driver remained active")
    declared_post_release_frames = _parse_nonnegative_int(
        fields["post_release_frames"], "post_release_frames"
    )
    frames = tuple(
        _parse_post_release_frame(encoded, index)
        for index, encoded in enumerate(fields["post_release_sequence"].split(";"))
    )
    if declared_post_release_frames != len(frames):
        raise RuntimeError(
            "declared post-release frame count does not match the complete sequence"
        )
    _validate_post_release_sequence(frames)

    evidence = FrameClockEvidence(
        callback_entries=_parse_nonnegative_int(
            fields["callback_entries"], "callback_entries"
        ),
        callback_registrations=_parse_nonnegative_int(
            fields["callback_registrations"], "callback_registrations"
        ),
        frame_requests=_parse_nonnegative_int(
            fields["frame_requests"], "frame_requests"
        ),
        pan_samples=_parse_nonnegative_int(fields["pan_samples"], "pan_samples"),
        pan_delta_x=_parse_finite_float(fields["pan_delta_x"], "pan_delta_x"),
        settle_frames=settle_frames,
        redraw_requests=redraw_requests,
        post_release_frames=frames,
    )
    if evidence.callback_entries <= 0:
        raise RuntimeError("production frame callback never entered")
    if evidence.callback_registrations < evidence.callback_entries:
        raise RuntimeError("callback entry count exceeds callback registrations")
    if evidence.frame_requests <= 0:
        raise RuntimeError("host did not request Open GPUI animation frames")
    if evidence.pan_samples < 3:
        raise RuntimeError("viewport did not move across three observed native frames")
    if evidence.pan_delta_x <= 0.0:
        raise RuntimeError("left-edge auto-pan did not move the viewport")
    first_post_release = evidence.post_release_frames[0]
    if first_post_release.callback_entries < evidence.callback_entries:
        raise RuntimeError("post-release callback entries precede active-frame evidence")
    if first_post_release.callback_registrations < evidence.callback_registrations:
        raise RuntimeError("post-release registrations precede active-frame evidence")
    if first_post_release.frame_requests < evidence.frame_requests:
        raise RuntimeError("post-release frame requests precede active-frame evidence")
    return evidence


def run_witness(binary: Path, timeout_seconds: float) -> FrameClockEvidence:
    if not binary.is_file():
        raise RuntimeError(f"frame-clock witness binary does not exist: {binary}")
    try:
        completed = run_process(
            [str(binary)],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout_seconds=timeout_seconds,
        )
    except subprocess.TimeoutExpired as error:
        if error.stdout:
            sys.stdout.write(error.stdout)
        if error.stderr:
            sys.stderr.write(error.stderr)
        raise RuntimeError(
            f"frame-clock witness timed out after {timeout_seconds:g} seconds"
        ) from error
    if completed.stdout:
        sys.stdout.write(completed.stdout)
    if completed.stderr:
        sys.stderr.write(completed.stderr)
    if completed.returncode != 0:
        raise RuntimeError(
            f"frame-clock witness exited with status {completed.returncode}"
        )
    return parse_pass_marker(completed.stdout or "")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument(
        "--timeout-seconds", type=float, default=DEFAULT_TIMEOUT_SECONDS
    )
    parser.add_argument(
        "--require-native",
        action="store_true",
        help="fail instead of skipping when the current host is not macOS",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.timeout_seconds <= 0:
        print("--timeout-seconds must be positive", file=sys.stderr)
        return 2
    if platform.system() != "Darwin":
        if args.require_native:
            print("the native Open GPUI frame-clock witness requires macOS", file=sys.stderr)
            return 2
        print("skipped native Open GPUI frame-clock witness: host is not macOS")
        return 0
    try:
        evidence = run_witness(args.binary.resolve(), args.timeout_seconds)
    except RuntimeError as error:
        print(error, file=sys.stderr)
        return TIMEOUT_EXIT_CODE if "timed out" in str(error) else 1
    print(
        "validated native Open GPUI frame clock: "
        f"callbacks={evidence.callback_entries} "
        f"pan_samples={evidence.pan_samples} "
        f"pan_delta_x={evidence.pan_delta_x:g}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
