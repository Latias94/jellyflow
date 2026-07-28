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
from uuid import UUID


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

COMMON_MARKER_FIELDS = {
    "status",
    "demo",
    "bounds",
    "scenario",
    "target_zoom",
    "reason",
    "initial_nodes",
    "initial_ports",
    "initial_edges",
    "initial_node_ids",
    "initial_port_ids",
    "initial_edge_ids",
    "surface_width",
    "surface_height",
    "visible_expected_nodes",
    "active_nodes",
    "parked_nodes",
    "mounts",
    "restores",
    "evictions",
    "hard_retirements",
    "protected_instances",
    "protected_leases",
    "protected_overflows",
    "eligible_parked_nodes",
    "strong_instances",
    "frame_materialization_overflows",
    "feedback_submissions",
    "feedback_failures",
    "layout_rejections",
    "measurement_failures",
    "geometry_layout_revision",
    "current_geometry_nodes",
    "current_geometry_ports",
    "current_geometry_edges",
    "current_geometry_node_ids",
    "current_geometry_port_ids",
    "current_geometry_edge_ids",
    "geometry_fresh_nodes",
    "geometry_rendered_ports",
    "geometry_routed_edges",
    "geometry_node_regions",
    "paint_image_width",
    "paint_image_height",
    "paint_canvas_width",
    "paint_non_background_pixels",
    "paint_distinct_colors",
    "paint_nodes",
    "input_evidence",
    "input_final_frame_validated",
    "input_bounds",
    "input_bounds_verified",
    "native_probe_failure",
    "native_probe_phase",
    "native_probe_wait_ticks",
    "diagnostic",
}
INTERACTION_MARKER_FIELDS = {
    "input_dispatched_events",
    "input_autopan_steering_events",
    "input_target_zoom_events",
    "input_target_zoom_captures",
    "input_node_dragged",
    "input_autopan_frames",
    "input_selection_chrome_fitted",
    "input_selection_chrome_sides",
    "input_selection_chrome_detached_run",
    "input_connection_autopan_frames",
    "input_tab_focus_moved",
    "input_tab_focus_restored",
    "input_viewport_panned",
    "input_viewport_zoomed",
    "input_connection_created",
    "input_connection_preview_wire_painted",
    "input_connection_release_frame_painted",
    "input_connection_release_continuous",
    "input_keyboard_edge_deleted",
    "input_reconnect_applied",
    "input_reconnect_preview_wire_painted",
    "input_reconnect_release_frame_painted",
    "input_reconnect_release_continuous",
    "input_search_opened",
    "input_search_focus_moved",
    "input_search_cancelled",
    "input_search_focus_restored",
}
LOD_MARKER_FIELDS = {
    "input_lod_samples",
    "input_lod_continuous",
    "input_lod_max_jump_in_view",
}
MARKER_FIELDS = COMMON_MARKER_FIELDS | INTERACTION_MARKER_FIELDS | LOD_MARKER_FIELDS
INTERACTION_TRUE_FIELDS = {
    "input_node_dragged",
    "input_selection_chrome_fitted",
    "input_tab_focus_moved",
    "input_tab_focus_restored",
    "input_viewport_panned",
    "input_viewport_zoomed",
    "input_connection_created",
    "input_connection_preview_wire_painted",
    "input_connection_release_frame_painted",
    "input_connection_release_continuous",
    "input_keyboard_edge_deleted",
    "input_reconnect_applied",
    "input_reconnect_preview_wire_painted",
    "input_reconnect_release_frame_painted",
    "input_reconnect_release_continuous",
    "input_search_opened",
    "input_search_focus_moved",
    "input_search_cancelled",
    "input_search_focus_restored",
}
EXPECTED_SURFACE_SIZES = {
    "compact": (680.0, 640.0),
    "normal": (1160.0, 900.0),
}
SURFACE_SIZE_TOLERANCE = 2.0
MIN_CANVAS_NON_BACKGROUND_PIXELS = 256
MIN_CANVAS_DISTINCT_COLORS = 4
MIN_NODE_NON_BACKGROUND_PIXELS = 16
MIN_AUTOPAN_FRAMES = 3
INTERACTION_EVENT_COUNT = 36
INTERACTION_CAPTURE_COUNT = 9
LOD_SAMPLE_COUNT = 3


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


def _marker_int(
    fields: Mapping[str, str], key: str, errors: list[str], *, minimum: int = 0
) -> int | None:
    try:
        value = int(fields[key])
    except (KeyError, ValueError):
        errors.append(f"marker {key} is not an integer: {fields.get(key)!r}")
        return None
    if value < minimum:
        errors.append(f"marker {key} is {value}, expected at least {minimum}")
        return None
    return value


def _marker_float(
    fields: Mapping[str, str], key: str, errors: list[str], *, minimum: float = 0.0
) -> float | None:
    try:
        value = float(fields[key])
    except (KeyError, ValueError):
        errors.append(f"marker {key} is not a number: {fields.get(key)!r}")
        return None
    if not math.isfinite(value) or value < minimum:
        errors.append(
            f"marker {key} is {fields.get(key)!r}, expected a finite value >= {minimum}"
        )
        return None
    return value


def _marker_uuid(raw: str, context: str, errors: list[str]) -> UUID | None:
    try:
        identity = UUID(raw)
    except ValueError:
        errors.append(f"marker {context} is not a canonical UUID: {raw!r}")
        return None
    if str(identity) != raw:
        errors.append(f"marker {context} is not a canonical UUID: {raw!r}")
        return None
    return identity


def _marker_ids(
    fields: Mapping[str, str], key: str, expected_count: int | None, errors: list[str]
) -> set[UUID] | None:
    raw = fields.get(key, "")
    if not raw or raw == "none":
        errors.append(f"marker {key} contains no identities")
        return None
    values: list[UUID] = []
    for value in raw.split(","):
        identity = _marker_uuid(value, f"{key} identity", errors)
        if identity is None:
            return None
        values.append(identity)
    identities = set(values)
    if len(identities) != len(values):
        errors.append(f"marker {key} contains duplicate identities")
    if expected_count is not None and len(values) != expected_count:
        errors.append(
            f"marker {key} contains {len(values)} identities, expected {expected_count}"
        )
    return identities


def _marker_node_regions(
    fields: Mapping[str, str], expected_count: int | None, errors: list[str]
) -> set[UUID] | None:
    raw = fields.get("geometry_node_regions", "")
    identities: list[UUID] = []
    if raw and raw != "none":
        for encoded in raw.split(","):
            parts = encoded.split(":")
            if len(parts) != 5:
                errors.append(
                    f"marker geometry_node_regions entry is malformed: {encoded!r}"
                )
                return None
            node = _marker_uuid(
                parts[0], "geometry_node_regions node identity", errors
            )
            if node is None:
                return None
            try:
                left, top, right, bottom = map(int, parts[1:])
            except ValueError:
                errors.append(
                    f"marker geometry_node_regions entry is malformed: {encoded!r}"
                )
                return None
            if right <= left or bottom <= top:
                errors.append(f"marker geometry_node_regions entry is empty: {encoded!r}")
            identities.append(node)
    unique = set(identities)
    if len(unique) != len(identities):
        errors.append("marker geometry_node_regions contains duplicate nodes")
    if expected_count is not None and len(identities) != expected_count:
        errors.append(
            f"marker geometry_node_regions contains {len(identities)} nodes, "
            f"expected {expected_count}"
        )
    return unique


def _marker_paint_nodes(
    fields: Mapping[str, str], expected_count: int | None, errors: list[str]
) -> set[UUID] | None:
    raw = fields.get("paint_nodes", "")
    identities: list[UUID] = []
    if raw and raw != "none":
        for encoded in raw.split(","):
            node_text, separator, counts = encoded.partition(":")
            parts = counts.split("/")
            if not separator or len(parts) != 3:
                errors.append(f"marker paint_nodes entry is malformed: {encoded!r}")
                return None
            node = _marker_uuid(node_text, "paint_nodes node identity", errors)
            if node is None:
                return None
            try:
                non_background, required, sampled = map(int, parts)
            except ValueError:
                errors.append(f"marker paint_nodes entry is malformed: {encoded!r}")
                return None
            if required < MIN_NODE_NON_BACKGROUND_PIXELS:
                errors.append(
                    f"marker paint_nodes entry requires only {required} pixels: {encoded!r}"
                )
            if non_background < required or sampled < non_background:
                errors.append(
                    f"marker paint_nodes entry is blank or inconsistent: {encoded!r}"
                )
            identities.append(node)
    unique = set(identities)
    if len(unique) != len(identities):
        errors.append("marker paint_nodes contains duplicate nodes")
    if expected_count is not None and len(identities) != expected_count:
        errors.append(
            f"marker paint_nodes contains {len(identities)} nodes, expected {expected_count}"
        )
    return unique


def _expect_marker_values(
    fields: Mapping[str, str], expected: Mapping[str, str], errors: list[str]
) -> None:
    for key, expected_value in expected.items():
        actual = fields.get(key)
        if actual != expected_value:
            errors.append(f"marker {key} is {actual!r}, expected {expected_value!r}")


def _validate_common_marker_fields(
    spec: RunSpec, fields: Mapping[str, str], errors: list[str]
) -> None:
    _expect_marker_values(
        fields,
        {
            "status": "pass",
            "demo": spec.demo,
            "bounds": spec.bounds,
            "scenario": spec.scenario,
            "target_zoom": spec.zoom,
            "reason": "complete_frame",
            "input_evidence": spec.scenario,
            "input_final_frame_validated": "true",
            "input_bounds": spec.bounds,
            "input_bounds_verified": "true",
            "native_probe_failure": "none",
            "native_probe_phase": "Complete",
            "native_probe_wait_ticks": "0",
            "diagnostic": "none",
        },
        errors,
    )

    initial_nodes = _marker_int(fields, "initial_nodes", errors, minimum=1)
    initial_ports = _marker_int(fields, "initial_ports", errors, minimum=1)
    initial_edges = _marker_int(fields, "initial_edges", errors, minimum=1)
    visible_nodes = _marker_int(fields, "visible_expected_nodes", errors)
    active_nodes = _marker_int(fields, "active_nodes", errors)
    parked_nodes = _marker_int(fields, "parked_nodes", errors)
    mounts = _marker_int(fields, "mounts", errors, minimum=1)
    strong_instances = _marker_int(fields, "strong_instances", errors, minimum=1)
    protected_instances = _marker_int(fields, "protected_instances", errors)
    protected_leases = _marker_int(fields, "protected_leases", errors)
    _marker_int(fields, "restores", errors)
    eligible_parked_nodes = _marker_int(fields, "eligible_parked_nodes", errors)
    for key in (
        "evictions",
        "hard_retirements",
        "protected_overflows",
        "frame_materialization_overflows",
        "feedback_failures",
        "layout_rejections",
        "measurement_failures",
    ):
        value = _marker_int(fields, key, errors)
        if value is not None and value != 0:
            errors.append(f"marker {key} is {value}, expected 0")
    _marker_int(fields, "feedback_submissions", errors)
    _marker_int(fields, "geometry_layout_revision", errors, minimum=1)

    if initial_nodes is not None:
        for key, value in (
            ("mounts", mounts),
            ("strong_instances", strong_instances),
        ):
            if value is not None and value != initial_nodes:
                errors.append(f"marker {key} is {value}, expected {initial_nodes}")
        if visible_nodes is not None and visible_nodes > initial_nodes:
            errors.append(
                f"marker visible_expected_nodes is {visible_nodes}, "
                f"expected at most {initial_nodes}"
            )
        if active_nodes is not None and parked_nodes is not None:
            retained_nodes = active_nodes + parked_nodes
            if retained_nodes != initial_nodes:
                errors.append(
                    f"marker retained node count is {retained_nodes}, "
                    f"expected {initial_nodes}"
                )
        if (
            active_nodes is not None
            and visible_nodes is not None
            and active_nodes < visible_nodes
        ):
            errors.append(
                f"marker active_nodes is {active_nodes}, "
                f"expected at least {visible_nodes} visible nodes"
            )
        if (
            parked_nodes is not None
            and visible_nodes is not None
            and parked_nodes > initial_nodes - min(visible_nodes, initial_nodes)
        ):
            errors.append(
                f"marker parked_nodes is {parked_nodes}, expected at most "
                f"{initial_nodes - min(visible_nodes, initial_nodes)}"
            )
    if (
        protected_instances is not None
        and active_nodes is not None
        and protected_instances > active_nodes
    ):
        errors.append("marker protected_instances exceeds active_nodes")
    if (
        protected_leases is not None
        and protected_instances is not None
        and protected_leases < protected_instances
    ):
        errors.append("marker protected_leases is smaller than protected_instances")
    if (
        eligible_parked_nodes is not None
        and parked_nodes is not None
        and eligible_parked_nodes > parked_nodes
    ):
        errors.append("marker eligible_parked_nodes exceeds parked_nodes")

    expected_surface = EXPECTED_SURFACE_SIZES[spec.bounds]
    surface_width = _marker_float(fields, "surface_width", errors, minimum=1.0)
    surface_height = _marker_float(fields, "surface_height", errors, minimum=1.0)
    for key, actual, expected_value in (
        ("surface_width", surface_width, expected_surface[0]),
        ("surface_height", surface_height, expected_surface[1]),
    ):
        if actual is not None and abs(actual - expected_value) > SURFACE_SIZE_TOLERANCE:
            errors.append(f"marker {key} is {actual}, expected {expected_value} +/- 2")

    current_nodes = _marker_int(fields, "current_geometry_nodes", errors, minimum=1)
    current_ports = _marker_int(fields, "current_geometry_ports", errors, minimum=1)
    current_edges = _marker_int(fields, "current_geometry_edges", errors, minimum=1)
    fresh_nodes = _marker_int(fields, "geometry_fresh_nodes", errors)
    rendered_ports = _marker_int(fields, "geometry_rendered_ports", errors)
    routed_edges = _marker_int(fields, "geometry_routed_edges", errors)
    for key, actual, upper_bound in (
        ("geometry_fresh_nodes", fresh_nodes, current_nodes),
        ("geometry_rendered_ports", rendered_ports, current_ports),
        ("geometry_routed_edges", routed_edges, current_edges),
    ):
        if actual is not None and upper_bound is not None and actual > upper_bound:
            errors.append(f"marker {key} is {actual}, expected at most {upper_bound}")
    if (
        fresh_nodes is not None
        and visible_nodes is not None
        and fresh_nodes < visible_nodes
    ):
        errors.append(
            f"marker geometry_fresh_nodes is {fresh_nodes}, "
            f"expected at least {visible_nodes} visible nodes"
        )

    initial_node_ids = _marker_ids(fields, "initial_node_ids", initial_nodes, errors)
    initial_port_ids = _marker_ids(fields, "initial_port_ids", initial_ports, errors)
    _marker_ids(fields, "initial_edge_ids", initial_edges, errors)
    current_node_ids = _marker_ids(
        fields, "current_geometry_node_ids", current_nodes, errors
    )
    current_port_ids = _marker_ids(
        fields, "current_geometry_port_ids", current_ports, errors
    )
    _marker_ids(fields, "current_geometry_edge_ids", current_edges, errors)
    if initial_node_ids is not None and current_node_ids != initial_node_ids:
        errors.append("marker current node identities differ from the initial graph")
    if initial_port_ids is not None and current_port_ids != initial_port_ids:
        errors.append("marker current port identities differ from the initial graph")
    region_ids = _marker_node_regions(fields, visible_nodes, errors)
    paint_node_ids = _marker_paint_nodes(fields, visible_nodes, errors)
    if (
        current_node_ids is not None
        and region_ids is not None
        and not region_ids.issubset(current_node_ids)
    ):
        errors.append("marker node-region identities differ from current geometry")
    if (
        current_node_ids is not None
        and paint_node_ids is not None
        and not paint_node_ids.issubset(current_node_ids)
    ):
        errors.append("marker paint-node identities differ from current geometry")
    if region_ids is not None and paint_node_ids != region_ids:
        errors.append("marker paint-node identities differ from visible node regions")
    if (
        visible_nodes == 0
        and routed_edges is not None
        and routed_edges == 0
    ):
        errors.append("marker final geometry contains no visible node or routed edge")

    image_width = _marker_int(fields, "paint_image_width", errors, minimum=1)
    image_height = _marker_int(fields, "paint_image_height", errors, minimum=1)
    canvas_width = _marker_int(fields, "paint_canvas_width", errors, minimum=1)
    _marker_int(
        fields,
        "paint_non_background_pixels",
        errors,
        minimum=MIN_CANVAS_NON_BACKGROUND_PIXELS,
    )
    _marker_int(
        fields,
        "paint_distinct_colors",
        errors,
        minimum=MIN_CANVAS_DISTINCT_COLORS,
    )
    if image_width is not None and canvas_width is not None and canvas_width > image_width:
        errors.append("marker paint_canvas_width exceeds paint_image_width")
    if image_height is not None and surface_height is not None and image_height < surface_height:
        errors.append("marker paint_image_height is smaller than the logical surface height")


def _validate_scenario_marker_fields(
    spec: RunSpec, fields: Mapping[str, str], errors: list[str]
) -> None:
    if spec.scenario == "interaction":
        _expect_marker_values(
            fields,
            {
                "input_target_zoom_captures": str(INTERACTION_CAPTURE_COUNT),
                "input_selection_chrome_sides": "4",
                "input_lod_samples": "missing",
                "input_lod_continuous": "missing",
                "input_lod_max_jump_in_view": "missing",
                **{key: "true" for key in INTERACTION_TRUE_FIELDS},
            },
            errors,
        )
        steering_events = _marker_int(
            fields, "input_autopan_steering_events", errors
        )
        dispatched_events = _marker_int(fields, "input_dispatched_events", errors)
        target_zoom_events = _marker_int(fields, "input_target_zoom_events", errors)
        if (
            steering_events is not None
            and dispatched_events is not None
            and dispatched_events != INTERACTION_EVENT_COUNT + steering_events
        ):
            errors.append(
                "marker input_dispatched_events does not equal the scripted event count "
                "plus input_autopan_steering_events"
            )
        if (
            target_zoom_events is not None
            and dispatched_events is not None
            and target_zoom_events != dispatched_events
        ):
            errors.append(
                "marker input_target_zoom_events differs from input_dispatched_events"
            )
        _marker_int(fields, "input_autopan_frames", errors, minimum=MIN_AUTOPAN_FRAMES)
        _marker_int(
            fields,
            "input_connection_autopan_frames",
            errors,
            minimum=MIN_AUTOPAN_FRAMES,
        )
        _marker_int(fields, "input_selection_chrome_detached_run", errors)
        return

    _expect_marker_values(
        fields,
        {
            **{key: "missing" for key in INTERACTION_MARKER_FIELDS},
            "input_lod_samples": str(LOD_SAMPLE_COUNT),
            "input_lod_continuous": "true",
        },
        errors,
    )
    _marker_float(fields, "input_lod_max_jump_in_view", errors)


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

    actual_fields = set(fields)
    if actual_fields != MARKER_FIELDS:
        errors.append(
            "marker fields mismatch; "
            f"missing={sorted(MARKER_FIELDS - actual_fields)}; "
            f"unexpected={sorted(actual_fields - MARKER_FIELDS)}"
        )
        return errors

    _validate_common_marker_fields(spec, fields, errors)
    _validate_scenario_marker_fields(spec, fields, errors)
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
