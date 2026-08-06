#!/usr/bin/env python3
"""Compile public consumers against extracted candidate crate archives."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tarfile
import tempfile
import textwrap
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence

import tomllib


TOOLS_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_ROOT))
try:
    from run_with_timeout import run_process
finally:
    sys.path.pop(0)


ROOT = TOOLS_ROOT.parent
PUBLISHABLE_CRATES = tuple(
    crate
    for crate in (ROOT / "scripts" / "release-crates.txt").read_text(encoding="utf-8").splitlines()
    if crate
)
if not PUBLISHABLE_CRATES:
    raise RuntimeError("the ordered release crate set is empty")
CARGO_COMMAND_TIMEOUT_SECONDS = 20 * 60
LICENSE_FILES = ("LICENSE-MIT", "LICENSE-APACHE")


def is_jellyflow_release_namespace(name: str) -> bool:
    return name == "jellyflow" or name.startswith("jellyflow-")


def _run_cargo(
    command: Sequence[str],
    *,
    cwd: Path,
    capture_output: bool = False,
) -> subprocess.CompletedProcess[str] | subprocess.CompletedProcess[bytes]:
    return run_process(
        command,
        cwd=cwd,
        check=True,
        capture_output=capture_output,
        text=capture_output,
        timeout_seconds=CARGO_COMMAND_TIMEOUT_SECONDS,
    )


@dataclass(frozen=True)
class SmokeScenario:
    slug: str
    cargo_toml: str
    main_rs: str


@dataclass(frozen=True)
class PackageSmokeScenario:
    smoke: SmokeScenario
    expected_jellyflow_closure: frozenset[str]
    checks_open_gpui_features: bool = False


@dataclass(frozen=True)
class PackagedCrate:
    name: str
    version: str
    source_dir: Path


def write_file(path: Path, contents: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents, encoding="utf-8")


def _load_external_consumer_main_rs(fixture_name: str) -> str:
    fixture_path = (
        ROOT
        / "tools"
        / "fixtures"
        / "external-consumer"
        / fixture_name
        / "src/main.rs"
    )
    return fixture_path.read_text(encoding="utf-8")


def open_gpui_adoption_main_rs() -> str:
    """Return the public Open GPUI archive-consumer recipe."""

    return _load_external_consumer_main_rs("open-gpui-adoption")


def extract_package_archive(archive: Path, destination: Path) -> Path:
    with tarfile.open(archive, mode="r:gz") as package:
        members = package.getmembers()
        roots: set[str] = set()
        for member in members:
            member_path = Path(member.name)
            if member_path.is_absolute() or ".." in member_path.parts:
                raise RuntimeError(f"unsafe path in package archive {archive}: {member.name}")
            if member.issym() or member.islnk() or not (member.isfile() or member.isdir()):
                raise RuntimeError(
                    f"unsafe member type in package archive {archive}: {member.name}"
                )
            if member_path.parts:
                roots.add(member_path.parts[0])
        if len(roots) != 1:
            raise RuntimeError(
                f"package archive {archive} must contain one root directory, found {roots}"
            )
        try:
            package.extractall(destination, filter="data")
        except TypeError:
            # Python 3.11 lacks the extraction filter; paths and member types were validated above.
            package.extractall(destination)
    return destination / roots.pop()


def normalized_package_manifest(source_dir: Path) -> dict[str, object]:
    manifest_path = source_dir / "Cargo.toml"
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    if "workspace" in manifest:
        raise RuntimeError(f"normalized package manifest retained [workspace]: {manifest_path}")
    return manifest


def validate_packaged_licenses(repo_root: Path, source_dir: Path) -> None:
    for name in LICENSE_FILES:
        canonical = repo_root / name
        packaged = source_dir / name
        if not packaged.is_file():
            raise RuntimeError(f"package archive is missing {name}: {source_dir}")
        if packaged.read_bytes() != canonical.read_bytes():
            raise RuntimeError(
                f"package archive {name} does not match the workspace license: {source_dir}"
            )


def local_package_patch_args(repo_root: Path, crate: str) -> list[str]:
    """Resolve an unpublished release set while preserving packaged manifests."""

    args: list[str] = []
    for dependency in PUBLISHABLE_CRATES:
        if dependency == crate:
            continue
        dependency_path = repo_root / "crates" / dependency
        args.extend(
            [
                "--config",
                f"patch.crates-io.{dependency}.path={json.dumps(str(dependency_path))}",
            ]
        )
    return args


def assemble_package_archives(
    repo_root: Path, package_root: Path, crates: Sequence[str]
) -> list[PackagedCrate]:
    target_dir = package_root / "target"
    extracted_dir = package_root / "extracted"
    packaged: list[PackagedCrate] = []
    for crate in crates:
        crate_target_dir = target_dir / crate
        print(f"assembling package archive: {crate}", flush=True)
        command = [
            "cargo",
            "package",
            "--locked",
            "--allow-dirty",
            "--no-verify",
            "--package",
            crate,
            "--target-dir",
            str(crate_target_dir),
        ]
        command.extend(local_package_patch_args(repo_root, crate))
        _run_cargo(
            command,
            cwd=repo_root,
        )
        archives = sorted((crate_target_dir / "package").glob("*.crate"))
        if len(archives) != 1:
            raise RuntimeError(
                f"expected one package archive for {crate}, found {len(archives)}"
            )
        archive = archives[0]
        source_dir = extract_package_archive(archive, extracted_dir)
        validate_packaged_licenses(repo_root, source_dir)
        manifest = normalized_package_manifest(source_dir)
        package = manifest.get("package")
        if not isinstance(package, dict):
            raise RuntimeError(f"package metadata missing from {source_dir / 'Cargo.toml'}")
        name = package.get("name")
        version = package.get("version")
        if name != crate or not isinstance(version, str):
            raise RuntimeError(
                f"unexpected package identity in {source_dir / 'Cargo.toml'}: {name} {version}"
            )
        packaged.append(PackagedCrate(crate, version, source_dir))
    print("candidate archives assembled; archive-backed compile verification follows")
    return packaged


def package_archive_patch_config(
    packaged: Sequence[PackagedCrate], target_dir: Path
) -> str:
    lines = ["[patch.crates-io]"]
    lines.extend(
        f"{package.name} = {{ path = {json.dumps(str(package.source_dir.resolve()))} }}"
        for package in sorted(packaged, key=lambda package: package.name)
    )
    lines.extend(
        [
            "",
            "[build]",
            f"target-dir = {json.dumps(str(target_dir.resolve()))}",
        ]
    )
    return "\n".join(lines) + "\n"


def external_cargo_toml(slug: str, dependencies: list[str]) -> str:
    package = textwrap.dedent(
        f"""
        [package]
        name = "{slug}"
        version = "0.0.0"
        edition = "2024"
        rust-version = "1.95"
        publish = false
        """
    ).strip()
    return f"{package}\n\n[dependencies]\n" + "\n".join(dependencies) + "\n"


def packaged_jellyflow_dependency(name: str, versions: dict[str, str]) -> str:
    return (
        f'{name} = {{ version = "={versions[name]}", default-features = false }}'
    )


def jellyflow_package_consumer_scenarios(
    versions: dict[str, str],
) -> list[PackageSmokeScenario]:
    """Build isolated consumers from the temporary archive-backed Cargo source."""

    headless_direct = frozenset({"jellyflow-layout", "jellyflow-runtime"})
    headless_closure = frozenset(
        {"jellyflow-core", "jellyflow-layout", "jellyflow-runtime"}
    )
    egui_direct = frozenset(
        {"jellyflow-core", "jellyflow-egui", "jellyflow-runtime"}
    )
    egui_closure = frozenset(
        {"jellyflow-core", "jellyflow-egui", "jellyflow-runtime"}
    )
    open_gpui_direct = frozenset(
        {"jellyflow-core", "jellyflow-runtime", "jellyflow-open-gpui"}
    )
    open_gpui_closure = frozenset(
        {"jellyflow-core", "jellyflow-open-gpui", "jellyflow-runtime"}
    )

    return [
        PackageSmokeScenario(
            smoke=SmokeScenario(
                slug="jellyflow-package-headless-smoke",
                cargo_toml=external_cargo_toml(
                    "jellyflow-package-headless-smoke",
                    [
                        packaged_jellyflow_dependency(name, versions)
                        for name in sorted(headless_direct)
                    ],
                ),
                main_rs=_load_external_consumer_main_rs("package-headless"),
            ),
            expected_jellyflow_closure=headless_closure,
        ),
        PackageSmokeScenario(
            smoke=SmokeScenario(
                slug="jellyflow-package-egui-smoke",
                cargo_toml=external_cargo_toml(
                    "jellyflow-package-egui-smoke",
                    [
                        packaged_jellyflow_dependency(name, versions)
                        for name in sorted(egui_direct)
                    ],
                ),
                main_rs=_load_external_consumer_main_rs("package-egui"),
            ),
            expected_jellyflow_closure=egui_closure,
        ),
        PackageSmokeScenario(
            smoke=SmokeScenario(
                slug="jellyflow-package-open-gpui-smoke",
                cargo_toml=external_cargo_toml(
                    "jellyflow-package-open-gpui-smoke",
                    [
                        packaged_jellyflow_dependency(name, versions)
                        for name in sorted(open_gpui_direct)
                    ]
                    + [
                        'open_gpui = { package = "open-gpui", version = "=0.2.0", '
                        "default-features = false }",
                        'open_gpui_ui_components = { package = "open-gpui-ui-components", '
                        'version = "=0.2.0", default-features = false }',
                    ],
                ),
                main_rs=open_gpui_adoption_main_rs(),
            ),
            expected_jellyflow_closure=open_gpui_closure,
            checks_open_gpui_features=True,
        ),
    ]


def make_temp_root(keep: bool) -> tuple[Path, tempfile.TemporaryDirectory[str] | None]:
    if keep:
        return Path(tempfile.mkdtemp(prefix="jellyflow_external_smoke_")), None

    context = tempfile.TemporaryDirectory(prefix="jellyflow_external_smoke_")
    return Path(context.name), context


def write_scenario(project_dir: Path, scenario: SmokeScenario) -> None:
    write_file(project_dir / "Cargo.toml", scenario.cargo_toml)
    write_file(project_dir / "src/main.rs", scenario.main_rs)


def run_cargo_manifest(
    cwd: Path,
    manifest_path: Path,
    locked: bool = False,
) -> None:
    command = [
        "cargo",
        "run",
        "--quiet",
        "--manifest-path",
        str(manifest_path),
    ]
    if locked:
        command.append("--locked")
    _run_cargo(
        command,
        cwd=cwd,
    )


def cargo_metadata_for_manifest(
    cwd: Path,
    manifest_path: Path,
    *,
    locked: bool = False,
) -> dict[str, object]:
    command = [
        "cargo",
        "metadata",
        "--format-version",
        "1",
        "--manifest-path",
        str(manifest_path),
    ]
    if locked:
        command.append("--locked")
    result = _run_cargo(
        command,
        cwd=cwd,
        capture_output=True,
    )
    metadata = json.loads(result.stdout)
    if not isinstance(metadata, dict):
        raise RuntimeError("cargo metadata returned a non-object JSON value")
    return metadata


def resolved_package_features(
    metadata: dict[str, object],
    package_name: str,
) -> set[str]:
    package_ids = {
        package["id"]
        for package in metadata["packages"]
        if package["name"] == package_name
    }
    if not package_ids:
        raise RuntimeError(f"cargo metadata omitted package {package_name}")
    return {
        feature
        for node in metadata["resolve"]["nodes"]
        if node["id"] in package_ids
        for feature in node["features"]
    }


def package_names_in_metadata(metadata: dict[str, object]) -> set[str]:
    return {package["name"] for package in metadata["packages"]}


def assert_package_metadata_boundary(
    metadata: dict[str, object], scenario: PackageSmokeScenario
) -> set[str]:
    packages = package_names_in_metadata(metadata)
    allowed_jellyflow_packages = set(PUBLISHABLE_CRATES) | {scenario.smoke.slug}
    unexpected_jellyflow_packages = {
        package
        for package in packages
        if is_jellyflow_release_namespace(package)
        and package not in allowed_jellyflow_packages
    }
    if unexpected_jellyflow_packages:
        raise RuntimeError(
            f"{scenario.smoke.slug} resolved Jellyflow packages outside the release set: "
            f"{sorted(unexpected_jellyflow_packages)}"
        )
    jellyflow_packages = packages.intersection(PUBLISHABLE_CRATES)
    if jellyflow_packages != scenario.expected_jellyflow_closure:
        raise RuntimeError(
            f"{scenario.smoke.slug} resolved Jellyflow closure "
            f"{sorted(jellyflow_packages)}, expected "
            f"{sorted(scenario.expected_jellyflow_closure)}"
        )

    production_packages = set(jellyflow_packages)
    if scenario.checks_open_gpui_features:
        production_packages.add("open-gpui")
    test_support_packages = sorted(
        package
        for package in production_packages
        if "test-support" in resolved_package_features(metadata, package)
    )
    if test_support_packages:
        raise RuntimeError(
            f"{scenario.smoke.slug} activated test-support in production packages: "
            f"{test_support_packages}"
        )

    print(
        f"{scenario.smoke.slug} resolves exact Jellyflow closure: "
        + ", ".join(sorted(jellyflow_packages))
    )
    return jellyflow_packages


def assert_archive_metadata_sources(
    metadata: dict[str, object],
    packaged: Sequence[PackagedCrate],
    expected_names: frozenset[str],
    consumer_name: str,
) -> None:
    packaged_by_name = {package.name: package for package in packaged}
    missing_archives = expected_names.difference(packaged_by_name)
    if missing_archives:
        raise RuntimeError(
            f"{consumer_name} expected unassembled archives: {sorted(missing_archives)}"
        )
    packages = metadata.get("packages")
    if not isinstance(packages, list):
        raise RuntimeError("cargo metadata omitted packages")
    for name in sorted(expected_names):
        package = packaged_by_name[name]
        matches = [
            candidate
            for candidate in packages
            if candidate.get("name") == package.name
        ]
        if len(matches) != 1:
            raise RuntimeError(
                f"cargo metadata contains {len(matches)} candidates for "
                f"{package.name} {package.version}"
            )
        resolved = matches[0]
        if resolved.get("version") != package.version:
            raise RuntimeError(
                f"{package.name} resolved version {resolved.get('version')}, "
                f"expected {package.version}"
            )
        manifest_path = resolved.get("manifest_path")
        if not isinstance(manifest_path, str):
            raise RuntimeError(
                f"cargo metadata omitted manifest_path for {package.name} {package.version}"
            )
        resolved_source = Path(manifest_path).resolve().parent
        expected_source = package.source_dir.resolve()
        if resolved_source != expected_source:
            raise RuntimeError(
                f"{package.name} resolved from {resolved_source}, expected extracted archive "
                f"source {expected_source}"
            )


def run_package_consumer_smoke(
    project_dir: Path,
    packaged: list[PackagedCrate],
    scenario: PackageSmokeScenario,
    target_dir: Path,
) -> set[str]:
    write_scenario(project_dir, scenario.smoke)
    write_file(
        project_dir / ".cargo/config.toml",
        package_archive_patch_config(packaged, target_dir),
    )
    manifest = project_dir / "Cargo.toml"
    print(
        f"archive-backed {scenario.smoke.slug} project: {project_dir}", flush=True
    )
    _run_cargo(
        [
            "cargo",
            "generate-lockfile",
            "--manifest-path",
            str(manifest),
        ],
        cwd=project_dir,
    )
    metadata = cargo_metadata_for_manifest(project_dir, manifest, locked=True)
    jellyflow_closure = assert_package_metadata_boundary(metadata, scenario)
    assert_archive_metadata_sources(
        metadata,
        packaged,
        scenario.expected_jellyflow_closure,
        scenario.smoke.slug,
    )
    run_cargo_manifest(project_dir, manifest, locked=True)
    print(f"{scenario.smoke.slug} compiled from candidate archives")
    return jellyflow_closure


def run_package_consumers_smoke(
    project_root: Path,
    packaged: list[PackagedCrate],
) -> None:
    versions = {package.name: package.version for package in packaged}
    scenarios = jellyflow_package_consumer_scenarios(versions)
    resolved_union = set()
    target_dir = project_root / "target"
    for scenario in scenarios:
        resolved_union.update(
            run_package_consumer_smoke(
                project_root / scenario.smoke.slug,
                packaged,
                scenario,
                target_dir,
            )
        )
    expected_union = {
        package
        for scenario in scenarios
        for package in scenario.expected_jellyflow_closure
    }
    if resolved_union != expected_union:
        raise RuntimeError(
            "archive-backed consumer coverage mismatch: "
            f"resolved {sorted(resolved_union)}, expected {sorted(expected_union)}"
        )
    if expected_union != set(PUBLISHABLE_CRATES):
        raise RuntimeError(
            "archive-backed consumer definitions do not cover the complete release set: "
            f"{sorted(expected_union)}"
        )
    print("isolated package consumers cover the complete candidate archive set")


def parse_arguments(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Assemble candidate archives and compile the three public Jellyflow consumers "
            "against their extracted sources."
        )
    )
    parser.add_argument(
        "--keep",
        action="store_true",
        help="keep the temporary smoke projects for inspection",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_arguments(argv)

    repo_root = ROOT
    temp_root, temp_context = make_temp_root(args.keep)

    try:
        package_root = temp_root / "package-source"
        packaged = assemble_package_archives(
            repo_root, package_root, PUBLISHABLE_CRATES
        )
        run_package_consumers_smoke(
            temp_root / "package-consumers",
            packaged,
        )
        return 0
    finally:
        if args.keep:
            print(f"kept external smoke project: {temp_root}")
        elif temp_context is not None:
            temp_context.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())
