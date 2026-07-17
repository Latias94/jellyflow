from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "check_no_fret_dependencies.py"
SPEC = importlib.util.spec_from_file_location("check_no_fret_dependencies", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {SCRIPT_PATH}")
dependency_gate = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = dependency_gate
SPEC.loader.exec_module(dependency_gate)


def metadata_fixture(
    packages: list[tuple[str, str]],
    dependencies: dict[str, list[tuple[str, str]]],
) -> dict[str, object]:
    return {
        "packages": [
            {"id": package_id, "name": name, "version": "1.0.0"}
            for package_id, name in packages
        ],
        "workspace_members": ["runtime-id"],
        "resolve": {
            "nodes": [
                {
                    "id": package_id,
                    "deps": [
                        {"name": dependency_name, "pkg": dependency_id}
                        for dependency_name, dependency_id in dependencies.get(package_id, [])
                    ],
                }
                for package_id, _ in packages
            ]
        },
    }


class DependencyGraphTests(unittest.TestCase):
    def test_metadata_timeout_is_reported_as_a_bounded_gate_failure(self) -> None:
        with patch.object(
            dependency_gate.subprocess,
            "run",
            side_effect=dependency_gate.subprocess.TimeoutExpired(
                cmd=["cargo", "metadata"],
                timeout=dependency_gate.CARGO_METADATA_TIMEOUT_SECONDS,
            ),
        ):
            with self.assertRaisesRegex(
                RuntimeError,
                r"cargo metadata timed out after 120 seconds",
            ):
                dependency_gate.load_metadata(Path("."))

    def test_transitive_renamed_ui_dependency_reports_the_complete_path(self) -> None:
        metadata = metadata_fixture(
            [
                ("runtime-id", "jellyflow-runtime"),
                ("helper-id", "helper"),
                ("egui-id", "egui"),
            ],
            {
                "runtime-id": [("helper", "helper-id")],
                "helper-id": [("renamed_ui", "egui-id")],
            },
        )
        graph = dependency_gate.DependencyGraph.from_metadata(metadata)

        paths = graph.shortest_forbidden_paths(
            graph.workspace_package_id("jellyflow-runtime"),
            dependency_gate.is_headless_forbidden_package,
        )

        self.assertEqual(
            [[package.name for package in path] for path in paths],
            [["jellyflow-runtime", "helper", "egui"]],
        )

    def test_fret_policy_reaches_beyond_the_old_depth_two_limit(self) -> None:
        packages = [("runtime-id", "jellyflow-runtime")]
        dependencies: dict[str, list[tuple[str, str]]] = {}
        previous_id = "runtime-id"
        for depth in range(1, 5):
            package_id = f"helper-{depth}-id"
            package_name = f"helper-{depth}"
            packages.append((package_id, package_name))
            dependencies[previous_id] = [(package_name, package_id)]
            previous_id = package_id
        packages.append(("fret-id", "fret-ui"))
        dependencies[previous_id] = [("fret_ui", "fret-id")]
        graph = dependency_gate.DependencyGraph.from_metadata(
            metadata_fixture(packages, dependencies)
        )

        paths = graph.shortest_forbidden_paths(
            graph.workspace_package_id("jellyflow-runtime"),
            dependency_gate.is_fret_package,
        )

        self.assertEqual(paths[0][-1].name, "fret-ui")
        self.assertEqual(len(paths[0]), 6)

    def test_cycles_do_not_hide_or_duplicate_a_forbidden_dependency(self) -> None:
        metadata = metadata_fixture(
            [
                ("runtime-id", "jellyflow-runtime"),
                ("a-id", "helper-a"),
                ("b-id", "helper-b"),
                ("winit-id", "winit"),
            ],
            {
                "runtime-id": [("helper_a", "a-id")],
                "a-id": [("helper_b", "b-id")],
                "b-id": [("helper_a", "a-id"), ("winit", "winit-id")],
            },
        )
        graph = dependency_gate.DependencyGraph.from_metadata(metadata)

        paths = graph.shortest_forbidden_paths(
            graph.workspace_package_id("jellyflow-runtime"),
            dependency_gate.is_headless_forbidden_package,
        )

        self.assertEqual(len(paths), 1)
        self.assertEqual(paths[0][-1].name, "winit")

    def test_headless_policy_covers_framework_window_and_gpu_package_families(self) -> None:
        for package_name in (
            "accesskit",
            "egui",
            "open-gpui",
            "gpui",
            "winit",
            "raw-window-handle",
            "wgpu-core",
            "wayland-client",
        ):
            with self.subTest(package_name=package_name):
                self.assertTrue(
                    dependency_gate.is_headless_forbidden_package(package_name)
                )

    def test_similarly_named_non_ui_package_is_not_rejected(self) -> None:
        self.assertFalse(dependency_gate.is_headless_forbidden_package("iced-x86"))
        self.assertFalse(dependency_gate.is_fret_package("fretting"))


if __name__ == "__main__":
    unittest.main()
