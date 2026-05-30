#!/usr/bin/env python3
"""Check that Jellyflow workspace packages do not depend on Fret packages."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


DEFAULT_PACKAGES = ("jellyflow-core", "jellyflow-runtime")


def is_forbidden_package(line: str) -> bool:
    package_name = line.strip().split(" ", 1)[0]
    return package_name == "fret" or package_name.startswith("fret-")


def run_tree(repo_root: Path, package: str, depth: int) -> list[str]:
    tree = subprocess.run(
        [
            "cargo",
            "tree",
            "-p",
            package,
            "--depth",
            str(depth),
            "--prefix",
            "none",
        ],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )
    return tree.stdout.splitlines()


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check Jellyflow package dependency trees for accidental Fret packages."
    )
    parser.add_argument(
        "--package",
        action="append",
        choices=DEFAULT_PACKAGES,
        help="package to check; defaults to both Jellyflow packages",
    )
    parser.add_argument(
        "--depth",
        type=int,
        default=2,
        help="cargo tree depth to inspect",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[1]
    packages = tuple(args.package or DEFAULT_PACKAGES)
    failed = False

    for package in packages:
        lines = run_tree(repo_root, package, args.depth)
        forbidden = [line.strip() for line in lines if is_forbidden_package(line)]
        if forbidden:
            failed = True
            print(
                f"{package} pulled Fret packages:\n" + "\n".join(forbidden),
                file=sys.stderr,
            )
        else:
            print(f"{package}: no fret or fret-* packages within depth {args.depth}")

    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
