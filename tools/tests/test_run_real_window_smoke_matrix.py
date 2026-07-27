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


NODE_IDS = (
    "00000000-0000-0000-0000-000000007100",
    "00000000-0000-0000-0000-000000007200",
)
PORT_IDS = (
    "00000000-0000-0000-0000-000000008000",
    "00000000-0000-0000-0000-000000008001",
    "00000000-0000-0000-0000-000000008002",
    "00000000-0000-0000-0000-000000008003",
)
INITIAL_EDGE_ID = "00000000-0000-0000-0000-000000009000"
CURRENT_EDGE_ID = "00000000-0000-0000-0000-000000009001"
FOREIGN_NODE_ID = "ffffffff-ffff-ffff-ffff-ffffffffffff"


FAKE_SMOKE_SOURCE = r'''from __future__ import annotations

import os
import sys
import time
from pathlib import Path

sys.path.insert(0, os.environ["JELLYFLOW_SMOKE_TOOLS"])
import run_real_window_smoke_matrix as contract

NODE_IDS = (
    "00000000-0000-0000-0000-000000007100",
    "00000000-0000-0000-0000-000000007200",
)
PORT_IDS = (
    "00000000-0000-0000-0000-000000008000",
    "00000000-0000-0000-0000-000000008001",
    "00000000-0000-0000-0000-000000008002",
    "00000000-0000-0000-0000-000000008003",
)
INITIAL_EDGE_ID = "00000000-0000-0000-0000-000000009000"
CURRENT_EDGE_ID = "00000000-0000-0000-0000-000000009001"

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
    logical_width, logical_height = contract.EXPECTED_SURFACE_SIZES[bounds]
    image_width = int(logical_width + 280)
    image_height = int(logical_height)
    fields = {field: "missing" for field in contract.MARKER_FIELDS}
    fields.update(
        status="pass",
        demo=marker_demo,
        bounds=bounds,
        scenario=scenario,
        target_zoom=zoom,
        reason="complete_frame",
        initial_nodes="2",
        initial_ports="4",
        initial_edges="1",
        initial_node_ids=",".join(NODE_IDS),
        initial_port_ids=",".join(PORT_IDS),
        initial_edge_ids=INITIAL_EDGE_ID,
        surface_width=str(logical_width),
        surface_height=str(logical_height),
        visible_expected_nodes="2",
        active_nodes="2",
        parked_nodes="0",
        mounts="2",
        restores="0",
        evictions="0",
        hard_retirements="0",
        protected_instances="0",
        protected_leases="0",
        protected_overflows="0",
        eligible_parked_nodes="0",
        strong_instances="2",
        frame_materialization_overflows="0",
        feedback_submissions="2",
        feedback_failures="0",
        layout_rejections="0",
        measurement_failures="0",
        geometry_layout_revision="7",
        current_geometry_nodes="2",
        current_geometry_ports="4",
        current_geometry_edges="1",
        current_geometry_node_ids=",".join(NODE_IDS),
        current_geometry_port_ids=",".join(PORT_IDS),
        current_geometry_edge_ids=CURRENT_EDGE_ID,
        geometry_fresh_nodes="2",
        geometry_rendered_ports="4",
        geometry_routed_edges="1",
        geometry_node_regions=(
            f"{NODE_IDS[0]}:40:40:240:180,{NODE_IDS[1]}:300:220:520:380"
        ),
        paint_image_width=str(image_width),
        paint_image_height=str(image_height),
        paint_canvas_width=str(int(logical_width)),
        paint_non_background_pixels="400",
        paint_distinct_colors="4",
        paint_nodes=(
            f"{NODE_IDS[0]}:100/16/200,{NODE_IDS[1]}:120/16/220"
        ),
        input_evidence=scenario,
        input_final_frame_validated="true",
        input_bounds=bounds,
        input_bounds_verified="true",
        native_probe_failure="none",
        native_probe_phase="Complete",
        native_probe_wait_ticks="0",
        diagnostic="none",
    )
    if scenario == "interaction":
        fields.update(
            input_dispatched_events="36",
            input_autopan_steering_events="0",
            input_target_zoom_events="36",
            input_target_zoom_captures="9",
            input_autopan_frames="3",
            input_selection_chrome_sides="4",
            input_selection_chrome_detached_run="0",
            input_connection_autopan_frames="3",
            **{key: "true" for key in contract.INTERACTION_TRUE_FIELDS},
        )
    else:
        fields.update(
            input_lod_samples="3",
            input_lod_continuous="true",
            input_lod_max_jump_in_view="1.25",
        )
    if active_mode == "minimal":
        fields = {
            "status": "pass",
            "demo": marker_demo,
            "bounds": bounds,
            "scenario": scenario,
            "target_zoom": zoom,
        }
    elif active_mode == "zero-paint":
        fields["paint_non_background_pixels"] = "0"
    elif active_mode == "false-input":
        fields["input_node_dragged"] = "false"
    marker = "JELLYFLOW_REAL_WINDOW_SMOKE " + " ".join(
        f"{key}={value}" for key, value in sorted(fields.items())
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
        environment["JELLYFLOW_SMOKE_TOOLS"] = str(SCRIPT_PATH.parent)
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

    def marker_errors(
        self, spec: matrix.RunSpec, fields: dict[str, str]
    ) -> list[str]:
        marker = "JELLYFLOW_REAL_WINDOW_SMOKE " + " ".join(
            f"{key}={value}" for key, value in sorted(fields.items())
        )
        return matrix.marker_contract_errors(
            spec,
            exit_code=0,
            timed_out=False,
            marker_lines=[marker],
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

    def test_canonical_uuid_identity_schema_is_accepted(self) -> None:
        spec = matrix.RUN_SPECS[-1]

        run = self.execute(spec, mode="pass")
        fields = matrix.parse_marker_line(str(run["marker"]))

        self.assertEqual(run["status"], "pass")
        self.assertEqual(run["errors"], [])
        self.assertEqual(set(fields["initial_node_ids"].split(",")), set(NODE_IDS))
        self.assertEqual(set(fields["initial_port_ids"].split(",")), set(PORT_IDS))
        self.assertEqual(fields["initial_edge_ids"], INITIAL_EDGE_ID)
        self.assertEqual(fields["current_geometry_edge_ids"], CURRENT_EDGE_ID)

    def test_interaction_event_receipt_includes_discrete_autopan_steering(self) -> None:
        spec = matrix.RUN_SPECS[0]
        run = self.execute(spec, mode="pass")
        fields = matrix.parse_marker_line(str(run["marker"]))
        fields.update(
            input_autopan_steering_events="2",
            input_dispatched_events="38",
            input_target_zoom_events="38",
        )

        self.assertEqual(self.marker_errors(spec, fields), [])

        fields["input_dispatched_events"] = "37"
        self.assertIn(
            "marker input_dispatched_events does not equal the scripted event count "
            "plus input_autopan_steering_events",
            self.marker_errors(spec, fields),
        )

        fields["input_dispatched_events"] = "38"
        fields["input_target_zoom_events"] = "37"
        self.assertIn(
            "marker input_target_zoom_events differs from input_dispatched_events",
            self.marker_errors(spec, fields),
        )

    def test_malformed_or_noncanonical_uuid_identity_is_rejected(self) -> None:
        spec = matrix.RUN_SPECS[-1]
        run = self.execute(spec, mode="pass")
        valid_fields = matrix.parse_marker_line(str(run["marker"]))

        for invalid_identity in (
            "not-a-uuid",
            "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF",
        ):
            with self.subTest(invalid_identity=invalid_identity):
                fields = dict(valid_fields)
                fields["initial_node_ids"] = f"{invalid_identity},{NODE_IDS[1]}"

                self.assertIn(
                    "marker initial_node_ids identity is not a canonical UUID: "
                    f"{invalid_identity!r}",
                    self.marker_errors(spec, fields),
                )

    def test_uuid_keyed_regions_and_paint_nodes_reject_invalid_or_foreign_keys(
        self,
    ) -> None:
        spec = matrix.RUN_SPECS[-1]
        run = self.execute(spec, mode="pass")
        valid_fields = matrix.parse_marker_line(str(run["marker"]))

        cases = (
            (
                "geometry_node_regions",
                f"not-a-uuid:40:40:240:180,{NODE_IDS[1]}:300:220:520:380",
                "marker geometry_node_regions node identity is not a canonical UUID: "
                "'not-a-uuid'",
            ),
            (
                "paint_nodes",
                f"not-a-uuid:100/16/200,{NODE_IDS[1]}:120/16/220",
                "marker paint_nodes node identity is not a canonical UUID: "
                "'not-a-uuid'",
            ),
            (
                "geometry_node_regions",
                f"{FOREIGN_NODE_ID}:40:40:240:180,{NODE_IDS[1]}:300:220:520:380",
                "marker node-region identities differ from current geometry",
            ),
            (
                "paint_nodes",
                f"{FOREIGN_NODE_ID}:100/16/200,{NODE_IDS[1]}:120/16/220",
                "marker paint-node identities differ from current geometry",
            ),
        )
        for field, value, expected_error in cases:
            with self.subTest(field=field, value=value):
                fields = dict(valid_fields)
                fields[field] = value

                self.assertIn(expected_error, self.marker_errors(spec, fields))

    def test_virtualized_final_frame_accepts_only_the_visible_node_evidence(
        self,
    ) -> None:
        spec = matrix.RUN_SPECS[0]
        run = self.execute(spec, mode="pass")
        fields = matrix.parse_marker_line(str(run["marker"]))
        fields.update(
            visible_expected_nodes="1",
            active_nodes="1",
            parked_nodes="1",
            restores="3",
            eligible_parked_nodes="1",
            geometry_fresh_nodes="1",
            geometry_rendered_ports="2",
            geometry_node_regions=f"{NODE_IDS[0]}:40:40:240:180",
            paint_nodes=f"{NODE_IDS[0]}:100/16/200",
        )

        self.assertEqual(self.marker_errors(spec, fields), [])

    def test_route_only_final_frame_accepts_empty_node_regions_and_paint_nodes(
        self,
    ) -> None:
        spec = matrix.RUN_SPECS[0]
        run = self.execute(spec, mode="pass")
        fields = matrix.parse_marker_line(str(run["marker"]))
        fields.update(
            visible_expected_nodes="0",
            active_nodes="0",
            parked_nodes="2",
            restores="3",
            eligible_parked_nodes="2",
            geometry_fresh_nodes="0",
            geometry_rendered_ports="0",
            geometry_routed_edges="1",
            geometry_node_regions="none",
            paint_nodes="none",
        )

        self.assertEqual(self.marker_errors(spec, fields), [])

        fields["geometry_routed_edges"] = "0"
        self.assertIn(
            "marker final geometry contains no visible node or routed edge",
            self.marker_errors(spec, fields),
        )

    def test_virtualized_final_frame_rejects_resource_and_visible_set_drift(
        self,
    ) -> None:
        spec = matrix.RUN_SPECS[0]
        run = self.execute(spec, mode="pass")
        valid_fields = matrix.parse_marker_line(str(run["marker"]))
        valid_fields.update(
            visible_expected_nodes="1",
            active_nodes="1",
            parked_nodes="1",
            restores="3",
            eligible_parked_nodes="1",
            geometry_fresh_nodes="1",
            geometry_rendered_ports="2",
            geometry_node_regions=f"{NODE_IDS[0]}:40:40:240:180",
            paint_nodes=f"{NODE_IDS[0]}:100/16/200",
        )
        cases = (
            (
                {"active_nodes": "2", "parked_nodes": "1"},
                "marker retained node count is 3, expected 2",
            ),
            (
                {"restores": "-1"},
                "marker restores is -1, expected at least 0",
            ),
            (
                {"eligible_parked_nodes": "2"},
                "marker eligible_parked_nodes exceeds parked_nodes",
            ),
            (
                {"active_nodes": "0", "parked_nodes": "2"},
                "marker active_nodes is 0, expected at least 1 visible nodes",
            ),
            (
                {"geometry_fresh_nodes": "0"},
                "marker geometry_fresh_nodes is 0, expected at least 1 visible nodes",
            ),
            (
                {"geometry_node_regions": "none"},
                "marker geometry_node_regions contains 0 nodes, expected 1",
            ),
            (
                {"paint_nodes": f"{NODE_IDS[1]}:120/16/220"},
                "marker paint-node identities differ from visible node regions",
            ),
        )
        for changes, expected_error in cases:
            with self.subTest(changes=changes):
                fields = dict(valid_fields)
                fields.update(changes)

                self.assertIn(expected_error, self.marker_errors(spec, fields))

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

    def test_minimal_marker_and_false_or_zero_evidence_are_rejected(self) -> None:
        spec = matrix.RUN_SPECS[0]

        minimal = self.execute(spec, mode="minimal")
        zero_paint = self.execute(spec, mode="zero-paint")
        false_input = self.execute(spec, mode="false-input")

        self.assertEqual(minimal["status"], "fail")
        self.assertIn("marker fields mismatch", minimal["errors"][0])
        self.assertEqual(zero_paint["status"], "fail")
        self.assertIn(
            "marker paint_non_background_pixels is 0, expected at least 256",
            zero_paint["errors"],
        )
        self.assertEqual(false_input["status"], "fail")
        self.assertIn(
            "marker input_node_dragged is 'false', expected 'true'",
            false_input["errors"],
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
        self.assertRegex(
            workflows["release"],
            r"(?s)- name: Archived Open GPUI package consumer\s+run: >-\s+"
            r"python3 tools/check_external_consumer_smoke\.py\s+"
            r"--packages-only\s+--package-consumer open-gpui",
        )
        self.assertIn("timeout-minutes: 60", workflows["release"])


if __name__ == "__main__":
    unittest.main()
