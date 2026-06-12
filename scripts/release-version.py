#!/usr/bin/env python3
"""Release version checks used by GitHub Actions."""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PUBLISHABLE_CRATES = (
    "jellyflow-core",
    "jellyflow-layout",
    "jellyflow-runtime",
    "jellyflow",
)


def validate_semver(version: str) -> None:
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
        raise ValueError(f"unsupported SemVer release version: {version!r}")


def load_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def cargo_workspace_version() -> str:
    data = load_toml(ROOT / "Cargo.toml")
    return str(data["workspace"]["package"]["version"])


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

    actual = {"Cargo workspace": cargo_workspace_version()}
    actual.update({crate: crate_version(crate) for crate in PUBLISHABLE_CRATES})

    failed = False
    for name, actual_version in actual.items():
        if actual_version == version:
            print(f"{name}: {actual_version}")
            continue
        failed = True
        print(
            f"::error::{name} version {actual_version!r} does not match expected {version!r}",
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
        return check_versions(args.version)
    if args.command == "crate-version":
        if args.crate is None:
            parser.error("crate-version requires --crate")
        print(crate_version(args.crate))
        return 0
    raise AssertionError(args.command)


if __name__ == "__main__":
    raise SystemExit(main())
