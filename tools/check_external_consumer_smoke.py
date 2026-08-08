#!/usr/bin/env python3
"""Compile representative consumers against locally packaged Jellyflow crates."""

from __future__ import annotations

import argparse
import json
import subprocess
import tarfile
import tempfile
import textwrap
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tools" / "fixtures" / "external-consumer"
RELEASE_CRATES = tuple(
    name
    for name in (ROOT / "scripts" / "release-crates.txt")
    .read_text(encoding="utf-8")
    .splitlines()
    if name
)
CARGO_TIMEOUT_SECONDS = 20 * 60


@dataclass(frozen=True)
class PackagedCrate:
    name: str
    version: str
    source: Path


@dataclass(frozen=True)
class Consumer:
    name: str
    dependencies: tuple[str, ...]
    fixture: str


def run(command: Sequence[str], *, cwd: Path) -> None:
    subprocess.run(
        command,
        cwd=cwd,
        check=True,
        timeout=CARGO_TIMEOUT_SECONDS,
    )


def package_patch_args(crate: str) -> list[str]:
    arguments: list[str] = []
    for dependency in RELEASE_CRATES:
        if dependency == crate:
            continue
        arguments.extend(
            [
                "--config",
                f"patch.crates-io.{dependency}.path="
                f"{json.dumps(str(ROOT / 'crates' / dependency))}",
            ]
        )
    return arguments


def package_crate(crate: str, package_root: Path) -> PackagedCrate:
    target = package_root / "target" / crate
    command = [
        "cargo",
        "package",
        "--locked",
        "--allow-dirty",
        "--no-verify",
        "--package",
        crate,
        "--target-dir",
        str(target),
    ]
    command.extend(package_patch_args(crate))
    print(f"packaging {crate}", flush=True)
    run(command, cwd=ROOT)

    archives = list((target / "package").glob("*.crate"))
    if len(archives) != 1:
        raise RuntimeError(f"expected one archive for {crate}, found {len(archives)}")
    extracted = package_root / "extracted"
    with tarfile.open(archives[0], mode="r:gz") as archive:
        roots = {Path(member.name).parts[0] for member in archive.getmembers()}
        if len(roots) != 1:
            raise RuntimeError(f"archive for {crate} has an unexpected root layout")
        try:
            archive.extractall(extracted, filter="data")
        except TypeError:
            # Python 3.11 has no extraction filter; Cargo produced this local archive.
            archive.extractall(extracted)
    source = extracted / roots.pop()
    manifest = tomllib.loads((source / "Cargo.toml").read_text(encoding="utf-8"))
    version = manifest.get("package", {}).get("version")
    if not isinstance(version, str):
        raise RuntimeError(f"packaged manifest for {crate} has no version")
    return PackagedCrate(crate, version, source)


def consumers(versions: dict[str, str]) -> tuple[Consumer, ...]:
    dependency = lambda name: (
        f'{name} = {{ version = "={versions[name]}", default-features = false }}'
    )
    return (
        Consumer(
            "jellyflow-package-headless-smoke",
            tuple(dependency(name) for name in ("jellyflow-layout", "jellyflow-runtime")),
            "package-headless",
        ),
        Consumer(
            "jellyflow-package-egui-smoke",
            tuple(
                dependency(name)
                for name in ("jellyflow-core", "jellyflow-runtime", "jellyflow-egui")
            ),
            "package-egui",
        ),
        Consumer(
            "jellyflow-package-open-gpui-smoke",
            tuple(
                dependency(name)
                for name in (
                    "jellyflow-core",
                    "jellyflow-runtime",
                    "jellyflow-open-gpui",
                )
            )
            + (
                'open_gpui = { package = "open-gpui", version = "=0.2.0", '
                "default-features = false }",
                'open_gpui_ui_components = { package = "open-gpui-ui-components", '
                'version = "=0.2.0", default-features = false }',
            ),
            "open-gpui-adoption",
        ),
    )


def patch_config(packages: Sequence[PackagedCrate]) -> str:
    lines = ["[patch.crates-io]"]
    lines.extend(
        f'{package.name} = {{ path = {json.dumps(str(package.source))} }}'
        for package in packages
    )
    return "\n".join(lines) + "\n"


def write_consumer(
    root: Path,
    consumer: Consumer,
    packages: Sequence[PackagedCrate],
) -> Path:
    project = root / consumer.name
    (project / "src").mkdir(parents=True)
    (project / ".cargo").mkdir()
    manifest = textwrap.dedent(
        f"""
        [package]
        name = "{consumer.name}"
        version = "0.0.0"
        edition = "2024"
        rust-version = "1.95"
        publish = false

        [dependencies]
        {chr(10).join(consumer.dependencies)}
        """
    ).strip()
    (project / "Cargo.toml").write_text(f"{manifest}\n", encoding="utf-8")
    (project / "src" / "main.rs").write_text(
        (FIXTURES / consumer.fixture / "src" / "main.rs").read_text(encoding="utf-8"),
        encoding="utf-8",
    )
    (project / ".cargo" / "config.toml").write_text(
        patch_config(packages), encoding="utf-8"
    )
    return project


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--keep", action="store_true")
    args = parser.parse_args(argv)

    temporary = None if args.keep else tempfile.TemporaryDirectory(prefix="jellyflow-package-smoke-")
    root = (
        Path(tempfile.mkdtemp(prefix="jellyflow-package-smoke-"))
        if temporary is None
        else Path(temporary.name)
    )
    try:
        packages = [package_crate(crate, root / "packages") for crate in RELEASE_CRATES]
        versions = {package.name: package.version for package in packages}
        for consumer in consumers(versions):
            project = write_consumer(root / "consumers", consumer, packages)
            print(f"checking {consumer.name}", flush=True)
            run(
                [
                    "cargo",
                    "generate-lockfile",
                    "--manifest-path",
                    str(project / "Cargo.toml"),
                ],
                cwd=project,
            )
            run(
                [
                    "cargo",
                    "check",
                    "--locked",
                    "--manifest-path",
                    str(project / "Cargo.toml"),
                    "--target-dir",
                    str(ROOT / "target"),
                ],
                cwd=project,
            )
        return 0
    finally:
        if args.keep:
            print(f"kept package smoke workspace: {root}")
        elif temporary is not None:
            temporary.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())
