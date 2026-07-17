#!/usr/bin/env python3
"""Check Fret and headless dependency boundaries across complete Cargo graphs."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from collections import deque
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PACKAGES = tuple(
    crate
    for crate in (ROOT / "scripts" / "release-crates.txt").read_text(encoding="utf-8").splitlines()
    if crate
)
if not DEFAULT_PACKAGES:
    raise RuntimeError("the ordered release crate set is empty")

HEADLESS_PACKAGES = frozenset(
    ("jellyflow-core", "jellyflow-layout", "jellyflow-runtime")
)
CARGO_METADATA_TIMEOUT_SECONDS = 120

HEADLESS_FORBIDDEN_PACKAGE_NAMES = frozenset(
    (
        # UI frameworks and their common rendering primitives.
        "accesskit",
        "dioxus",
        "dioxus-desktop",
        "druid",
        "ecolor",
        "eframe",
        "egui",
        "emath",
        "epaint",
        "floem",
        "gpui",
        "gtk",
        "gtk4",
        "iced",
        "iced_core",
        "iced_futures",
        "iced_graphics",
        "iced_renderer",
        "iced_runtime",
        "iced_tiny_skia",
        "iced_wgpu",
        "iced_widget",
        "iced_winit",
        "makepad-widgets",
        "open-gpui",
        "open_gpui",
        "relm4",
        "slint",
        "tauri",
        "vizia",
        # Window, display-server, and native UI integration.
        "cocoa",
        "glfw",
        "glutin",
        "glutin-winit",
        "objc2-app-kit",
        "objc2-ui-kit",
        "raw-window-handle",
        "sdl2",
        "sdl3",
        "smithay-client-toolkit",
        "softbuffer",
        "winit",
        "x11",
        # GPU and renderer integration.
        "ash",
        "glium",
        "glow",
        "metal",
        "pixels",
        "skia-safe",
        "tiny-skia",
        "vulkano",
        "wgpu",
    )
)

HEADLESS_FORBIDDEN_PACKAGE_PREFIXES = (
    "accesskit_",
    "dioxus-",
    "egui_",
    "egui-",
    "gpui-",
    "gtk-",
    "gtk4-",
    "makepad-",
    "open-gpui-",
    "slint-",
    "tauri-",
    "wayland-",
    "wgpu-",
    "winit-",
    "x11-",
)


@dataclass(frozen=True)
class Package:
    package_id: str
    name: str
    version: str

    def display(self) -> str:
        return f"{self.name}@{self.version}"


@dataclass(frozen=True)
class DependencyGraph:
    packages: Mapping[str, Package]
    dependencies: Mapping[str, tuple[str, ...]]
    workspace_members: frozenset[str]

    @classmethod
    def from_metadata(cls, metadata: Mapping[str, Any]) -> DependencyGraph:
        raw_packages = metadata.get("packages")
        raw_workspace_members = metadata.get("workspace_members")
        raw_resolve = metadata.get("resolve")
        if not isinstance(raw_packages, list):
            raise ValueError("cargo metadata did not contain a packages array")
        if not isinstance(raw_workspace_members, list):
            raise ValueError("cargo metadata did not contain workspace_members")
        if not isinstance(raw_resolve, dict) or not isinstance(
            raw_resolve.get("nodes"), list
        ):
            raise ValueError("cargo metadata did not contain a resolved dependency graph")

        packages: dict[str, Package] = {}
        for raw_package in raw_packages:
            if not isinstance(raw_package, dict):
                raise ValueError("cargo metadata contained an invalid package entry")
            package_id = raw_package.get("id")
            name = raw_package.get("name")
            version = raw_package.get("version")
            if not all(isinstance(value, str) for value in (package_id, name, version)):
                raise ValueError("cargo metadata contained an incomplete package entry")
            packages[package_id] = Package(package_id, name, version)

        dependencies: dict[str, tuple[str, ...]] = {}
        for raw_node in raw_resolve["nodes"]:
            if not isinstance(raw_node, dict) or not isinstance(raw_node.get("id"), str):
                raise ValueError("cargo metadata contained an invalid resolve node")
            node_id = raw_node["id"]
            raw_deps = raw_node.get("deps")
            if not isinstance(raw_deps, list):
                raise ValueError(f"resolve node {node_id!r} did not contain deps")

            dependency_ids: set[str] = set()
            for raw_dep in raw_deps:
                if not isinstance(raw_dep, dict) or not isinstance(raw_dep.get("pkg"), str):
                    raise ValueError(f"resolve node {node_id!r} contained an invalid dependency")
                dependency_ids.add(raw_dep["pkg"])

            unknown = dependency_ids.difference(packages)
            if unknown:
                raise ValueError(
                    f"resolve node {node_id!r} referenced unknown packages: "
                    + ", ".join(sorted(unknown))
                )
            dependencies[node_id] = tuple(
                sorted(
                    dependency_ids,
                    key=lambda package_id: (
                        packages[package_id].name,
                        packages[package_id].version,
                        package_id,
                    ),
                )
            )

        workspace_members = frozenset(raw_workspace_members)
        unknown_members = workspace_members.difference(packages)
        if unknown_members:
            raise ValueError(
                "workspace_members referenced unknown packages: "
                + ", ".join(sorted(unknown_members))
            )
        return cls(packages, dependencies, workspace_members)

    def workspace_package_id(self, package_name: str) -> str:
        matches = [
            package_id
            for package_id in self.workspace_members
            if self.packages[package_id].name == package_name
        ]
        if len(matches) != 1:
            raise ValueError(
                f"expected exactly one workspace package named {package_name!r}, "
                f"found {len(matches)}"
            )
        return matches[0]

    def shortest_forbidden_paths(
        self,
        root_id: str,
        is_forbidden: Callable[[str], bool],
    ) -> list[tuple[Package, ...]]:
        queue: deque[tuple[str, tuple[str, ...]]] = deque(((root_id, (root_id,)),))
        visited = {root_id}
        violations: list[tuple[Package, ...]] = []

        while queue:
            package_id, path = queue.popleft()
            for dependency_id in self.dependencies.get(package_id, ()):
                if dependency_id in visited:
                    continue
                visited.add(dependency_id)
                dependency_path = (*path, dependency_id)
                dependency = self.packages[dependency_id]
                if is_forbidden(dependency.name):
                    violations.append(
                        tuple(self.packages[path_id] for path_id in dependency_path)
                    )
                else:
                    queue.append((dependency_id, dependency_path))

        return sorted(
            violations,
            key=lambda path: tuple(package.display() for package in path),
        )


def is_fret_package(package_name: str) -> bool:
    return package_name == "fret" or package_name.startswith("fret-")


def is_headless_forbidden_package(package_name: str) -> bool:
    return package_name in HEADLESS_FORBIDDEN_PACKAGE_NAMES or package_name.startswith(
        HEADLESS_FORBIDDEN_PACKAGE_PREFIXES
    )


def load_metadata(repo_root: Path) -> Mapping[str, Any]:
    command = [
        "cargo",
        "metadata",
        "--format-version",
        "1",
        "--all-features",
        "--locked",
    ]
    try:
        result = subprocess.run(
            command,
            cwd=repo_root,
            capture_output=True,
            text=True,
            timeout=CARGO_METADATA_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired as error:
        raise RuntimeError(
            "cargo metadata timed out after "
            f"{CARGO_METADATA_TIMEOUT_SECONDS} seconds"
        ) from error
    if result.returncode != 0:
        details = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"cargo metadata failed: {details}")
    try:
        metadata = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"cargo metadata returned invalid JSON: {error}") from error
    if not isinstance(metadata, dict):
        raise RuntimeError("cargo metadata returned a non-object JSON value")
    return metadata


def print_violation(
    package_name: str,
    boundary: str,
    paths: Sequence[tuple[Package, ...]],
) -> None:
    print(f"{package_name} violates the {boundary} boundary:", file=sys.stderr)
    for path in paths:
        print(
            "  " + " -> ".join(package.display() for package in path),
            file=sys.stderr,
        )


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Check complete Jellyflow dependency graphs for Fret packages and keep "
            "headless crates free of UI, window, and GPU packages."
        )
    )
    parser.add_argument(
        "--package",
        action="append",
        choices=DEFAULT_PACKAGES,
        help="package to check; defaults to every publishable Jellyflow package",
    )
    args = parser.parse_args()

    packages = tuple(args.package or DEFAULT_PACKAGES)
    try:
        graph = DependencyGraph.from_metadata(load_metadata(ROOT))
        package_ids = {
            package_name: graph.workspace_package_id(package_name)
            for package_name in packages
        }
    except (RuntimeError, ValueError) as error:
        print(f"Dependency boundary gate could not run: {error}", file=sys.stderr)
        return 2

    failed = False
    for package_name, package_id in package_ids.items():
        fret_paths = graph.shortest_forbidden_paths(package_id, is_fret_package)
        if fret_paths:
            failed = True
            print_violation(package_name, "Fret dependency", fret_paths)
        else:
            print(f"{package_name}: complete dependency closure is free of Fret packages")

        if package_name in HEADLESS_PACKAGES:
            headless_paths = graph.shortest_forbidden_paths(
                package_id, is_headless_forbidden_package
            )
            if headless_paths:
                failed = True
                print_violation(
                    package_name,
                    "headless UI/window/GPU dependency",
                    headless_paths,
                )
            else:
                print(
                    f"{package_name}: complete dependency closure is free of "
                    "UI/window/GPU packages"
                )

    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
