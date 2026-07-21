#!/usr/bin/env python3
"""Release version checks used by GitHub Actions."""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PUBLISHABLE_CRATES = tuple(
    crate
    for crate in (ROOT / "scripts" / "release-crates.txt").read_text(encoding="utf-8").splitlines()
    if crate
)
if not PUBLISHABLE_CRATES:
    raise RuntimeError("the ordered release crate set is empty")
WORKSPACE_INTERNAL_DEPENDENCIES = (
    "jellyflow-core",
    "jellyflow-layout",
    "jellyflow-runtime",
)
PRODUCT_MANIFEST = ROOT / "examples" / "canvas-jellyflow" / "Cargo.toml"
PRODUCT_PACKAGE = "open-gpui-canvas-jellyflow"


def validate_semver(version: str) -> None:
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
        raise ValueError(f"unsupported SemVer release version: {version!r}")


def load_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def cargo_workspace_version() -> str:
    data = load_toml(ROOT / "Cargo.toml")
    version = data["workspace"]["package"]["version"]
    if not isinstance(version, str):
        raise ValueError("Cargo workspace package.version must be a string")
    return version


def workspace_dependency_version(dependency_name: str) -> str:
    data = load_toml(ROOT / "Cargo.toml")
    dependency = data["workspace"]["dependencies"].get(dependency_name)
    if isinstance(dependency, str):
        return dependency
    if isinstance(dependency, dict) and isinstance(dependency.get("version"), str):
        return dependency["version"]
    raise ValueError(
        f"workspace dependency {dependency_name!r} must declare a string version constraint"
    )


def product_version() -> str:
    data = load_toml(PRODUCT_MANIFEST)
    package = data["package"]
    package_name = package.get("name")
    if package_name != PRODUCT_PACKAGE:
        raise ValueError(
            f"{PRODUCT_MANIFEST.relative_to(ROOT)} package name {package_name!r} "
            f"does not match expected {PRODUCT_PACKAGE!r}"
        )
    version = package.get("version")
    if not isinstance(version, str):
        raise ValueError(f"{PRODUCT_PACKAGE} package.version must be a string")
    return version


def crate_version(crate_name: str) -> str:
    data = load_toml(ROOT / "crates" / crate_name / "Cargo.toml")
    package = data["package"]
    version = package.get("version")
    if isinstance(version, str):
        return version
    if isinstance(version, dict) and version.get("workspace") is True:
        return cargo_workspace_version()
    raise ValueError(f"{crate_name} does not have a supported package.version field")


def check_versions(version: str) -> int:
    validate_semver(version)

    actual = [("Cargo workspace version", cargo_workspace_version())]
    actual.extend(
        (f"{crate} package version", crate_version(crate))
        for crate in PUBLISHABLE_CRATES
    )
    actual.extend(
        (
            f"workspace dependency {dependency} version constraint",
            workspace_dependency_version(dependency),
        )
        for dependency in WORKSPACE_INTERNAL_DEPENDENCIES
    )
    actual.append((f"{PRODUCT_PACKAGE} package version", product_version()))

    failed = False
    for label, actual_version in actual:
        if actual_version == version:
            print(f"{label}: {actual_version}")
            continue
        failed = True
        print(
            f"::error::{label} {actual_version!r} does not match expected {version!r}",
            file=sys.stderr,
        )

    return 1 if failed else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["check", "crate-version"])
    parser.add_argument("--version")
    parser.add_argument("--crate")
    args = parser.parse_args()

    if args.command == "check":
        if args.version is None:
            parser.error("check requires --version")
        try:
            return check_versions(args.version)
        except (KeyError, TypeError, ValueError) as error:
            print(f"::error::{error}", file=sys.stderr)
            return 1
    if args.command == "crate-version":
        if args.crate is None:
            parser.error("crate-version requires --crate")
        try:
            print(crate_version(args.crate))
            return 0
        except (FileNotFoundError, KeyError, TypeError, ValueError) as error:
            print(f"::error::{error}", file=sys.stderr)
            return 1
    raise AssertionError(args.command)


if __name__ == "__main__":
    raise SystemExit(main())
