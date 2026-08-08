#!/usr/bin/env python3
"""Enforce Jellyflow dependency boundaries using Cargo's resolved tree."""

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RELEASE_CRATES = tuple(
    name
    for name in (ROOT / "scripts" / "release-crates.txt")
    .read_text(encoding="utf-8")
    .splitlines()
    if name
)
HEADLESS_CRATES = frozenset(
    ("jellyflow-core", "jellyflow-layout", "jellyflow-runtime")
)
HEADLESS_FORBIDDEN_NAMES = frozenset(
    (
        "accesskit",
        "dioxus",
        "eframe",
        "egui",
        "floem",
        "gpui",
        "gtk",
        "gtk4",
        "iced",
        "metal",
        "open-gpui",
        "raw-window-handle",
        "slint",
        "tauri",
        "wayland-client",
        "wgpu",
        "winit",
        "x11",
    )
)
HEADLESS_FORBIDDEN_PREFIXES = (
    "accesskit_",
    "dioxus-",
    "egui-",
    "egui_",
    "gpui-",
    "gtk-",
    "iced-",
    "open-gpui-",
    "slint-",
    "tauri-",
    "wayland-",
    "wgpu-",
    "winit-",
    "x11-",
)


def resolved_packages(package: str) -> set[str]:
    result = subprocess.run(
        [
            "cargo",
            "tree",
            "--package",
            package,
            "--all-features",
            "--edges",
            "normal,build",
            "--prefix",
            "none",
            "--no-dedupe",
            "--locked",
            "--format",
            "{p}",
        ],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
        timeout=120,
    )
    return {
        line.split(maxsplit=1)[0]
        for line in result.stdout.splitlines()
        if line.strip()
    }


def is_headless_forbidden(name: str) -> bool:
    return name in HEADLESS_FORBIDDEN_NAMES or name.startswith(
        HEADLESS_FORBIDDEN_PREFIXES
    )


def violations(package: str, dependencies: set[str]) -> list[str]:
    found = {
        dependency
        for dependency in dependencies
        if dependency == "fret" or dependency.startswith("fret-")
    }
    if package in HEADLESS_CRATES:
        found.update(filter(is_headless_forbidden, dependencies))
    if package == "jellyflow-egui" and "jellyflow-open-gpui" in dependencies:
        found.add("jellyflow-open-gpui")
    if package == "jellyflow-open-gpui" and "jellyflow-egui" in dependencies:
        found.add("jellyflow-egui")
    return sorted(found)


def main() -> int:
    failed = False
    for package in RELEASE_CRATES:
        found = violations(package, resolved_packages(package))
        if found:
            failed = True
            print(f"{package}: forbidden dependencies: {', '.join(found)}")
        else:
            print(f"{package}: dependency boundaries clean")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
