#!/usr/bin/env python3
"""Verify candidate archives and source-only native interaction from outside the workspace."""

from __future__ import annotations

import argparse
import functools
import hashlib
import http.server
import json
import shutil
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import threading
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Iterator, Sequence

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
CRATES_IO_INDEX = "https://github.com/rust-lang/crates.io-index"
CARGO_COMMAND_TIMEOUT_SECONDS = 20 * 60
PACKAGE_CONSUMER_NAMES = ("headless", "egui", "open-gpui")


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
    cargo_subcommand: str = "run"


@dataclass(frozen=True)
class PackageSmokeScenario:
    consumer_name: str
    smoke: SmokeScenario
    direct_jellyflow_dependencies: frozenset[str]
    direct_external_dependencies: frozenset[str]
    expected_jellyflow_closure: frozenset[str]
    forbidden_packages: frozenset[str]
    forbidden_package_families: tuple[str, ...] = ()


@dataclass(frozen=True)
class PackagedCrate:
    name: str
    version: str
    archive: Path
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
    """Return the one public Open GPUI recipe used by both consumer modes."""

    return _load_external_consumer_main_rs("open-gpui-adoption")


def sha256_file(path: Path) -> str:
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


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
    manifest_text = manifest_path.read_text(encoding="utf-8")
    manifest = tomllib.loads(manifest_text)
    if "workspace" in manifest:
        raise RuntimeError(f"normalized package manifest retained [workspace]: {manifest_path}")
    dependency_tables = [
        manifest.get("dependencies", {}),
        manifest.get("dev-dependencies", {}),
        manifest.get("build-dependencies", {}),
    ]
    targets = manifest.get("target", {})
    if isinstance(targets, dict):
        for target in targets.values():
            if isinstance(target, dict):
                dependency_tables.extend(
                    [
                        target.get("dependencies", {}),
                        target.get("dev-dependencies", {}),
                        target.get("build-dependencies", {}),
                    ]
                )
    for dependencies in dependency_tables:
        if not isinstance(dependencies, dict):
            continue
        for name, dependency in dependencies.items():
            package_name = (
                dependency.get("package", name)
                if isinstance(dependency, dict)
                else name
            )
            if (
                isinstance(package_name, str)
                and is_jellyflow_release_namespace(package_name)
                and package_name not in PUBLISHABLE_CRATES
            ):
                raise RuntimeError(
                    f"normalized dependency {name} names a Jellyflow package outside the release set "
                    f"{package_name} in {manifest_path}"
                )
            if isinstance(dependency, dict) and (
                "path" in dependency or "workspace" in dependency
            ):
                raise RuntimeError(
                    f"normalized dependency {name} retained path/workspace in {manifest_path}"
                )
    return manifest


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
    repo_root: Path, package_root: Path
) -> list[PackagedCrate]:
    target_dir = package_root / "target"
    extracted_dir = package_root / "extracted"
    packaged: list[PackagedCrate] = []
    for crate in PUBLISHABLE_CRATES:
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
        packaged.append(PackagedCrate(crate, version, archive, source_dir))
    print("candidate archives assembled; archive-backed compile verification follows")
    return packaged


def resolve_package_metadata(
    packaged: list[PackagedCrate],
) -> dict[str, dict[str, object]]:
    package_metadata: dict[str, dict[str, object]] = {}
    for package in packaged:
        manifest_path = package.source_dir / "Cargo.toml"
        result = _run_cargo(
            [
                "cargo",
                "metadata",
                "--no-deps",
                "--locked",
                "--offline",
                "--format-version",
                "1",
                "--manifest-path",
                str(manifest_path),
            ],
            cwd=package.source_dir,
            capture_output=True,
        )
        metadata = json.loads(result.stdout)
        candidates = [
            candidate
            for candidate in metadata["packages"]
            if candidate["name"] == package.name
            and candidate["version"] == package.version
        ]
        if len(candidates) != 1:
            raise RuntimeError(
                f"metadata returned {len(candidates)} candidates for "
                f"{package.name} {package.version}"
            )
        package_metadata[package.name] = candidates[0]
    missing = set(PUBLISHABLE_CRATES) - package_metadata.keys()
    if missing:
        raise RuntimeError(f"metadata omitted packaged crates: {sorted(missing)}")
    return package_metadata


def sparse_index_path(package_name: str) -> Path:
    name = package_name.lower()
    if len(name) == 1:
        return Path("1") / name
    if len(name) == 2:
        return Path("2") / name
    if len(name) == 3:
        return Path("3") / name[0] / name
    return Path(name[:2]) / name[2:4] / name


def dependency_index_entry(dependency: dict[str, object]) -> dict[str, object]:
    package_name = dependency["name"]
    rename = dependency["rename"]
    return {
        "name": rename or package_name,
        "req": dependency["req"],
        "features": dependency["features"],
        "optional": dependency["optional"],
        "default_features": dependency["uses_default_features"],
        "target": dependency["target"],
        "kind": dependency["kind"] or "normal",
        "registry": None
        if package_name in PUBLISHABLE_CRATES
        else dependency["registry"] or CRATES_IO_INDEX,
        "package": package_name if rename else None,
    }


def package_index_entry(
    package: PackagedCrate, metadata: dict[str, object]
) -> dict[str, object]:
    return {
        "name": package.name,
        "vers": package.version,
        "deps": [
            dependency_index_entry(dependency)
            for dependency in metadata["dependencies"]
        ],
        "cksum": sha256_file(package.archive),
        "features": metadata["features"],
        "yanked": False,
        "links": metadata["links"],
        "v": 2,
        "features2": {},
        "rust_version": metadata["rust_version"],
    }


def write_sparse_registry(
    root: Path,
    base_url: str,
    packaged: list[PackagedCrate],
    metadata: dict[str, dict[str, object]],
) -> None:
    write_file(
        root / "index/config.json",
        json.dumps({"dl": f"{base_url}/crates"}, separators=(",", ":")),
    )
    for package in packaged:
        index_entry = package_index_entry(package, metadata[package.name])
        write_file(
            root / "index" / sparse_index_path(package.name),
            json.dumps(index_entry, separators=(",", ":"), sort_keys=True) + "\n",
        )
        download = root / "crates" / package.name / package.version / "download"
        download.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(package.archive, download)


class QuietStaticHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:
        pass


@contextmanager
def serve_sparse_registry(
    root: Path,
    packaged: list[PackagedCrate],
    metadata: dict[str, dict[str, object]],
) -> Iterator[str]:
    handler = functools.partial(QuietStaticHandler, directory=str(root))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    server.daemon_threads = True
    host, port = server.server_address
    base_url = f"http://{host}:{port}"
    write_sparse_registry(root, base_url, packaged, metadata)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"sparse+{base_url}/index/"
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


def package_registry_config(index_url: str, target_dir: Path) -> str:
    return textwrap.dedent(
        f"""
        [registries.jellyflow-package-archives]
        index = {json.dumps(index_url)}

        [build]
        target-dir = {json.dumps(str(target_dir))}
        """
    ).strip() + "\n"


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


def jellyflow_source_path_smoke_scenarios(repo_root: Path) -> list[SmokeScenario]:
    """Keep framework test-support coverage separate from the package gate."""

    return [
        jellyflow_adapter_conformance_source_scenario(repo_root),
        jellyflow_open_gpui_source_scenario(repo_root),
    ]


def packaged_jellyflow_dependency(name: str, versions: dict[str, str]) -> str:
    return (
        f'{name} = {{ version = "={versions[name]}", '
        'registry = "jellyflow-package-archives" }'
    )


def jellyflow_package_consumer_scenarios(
    versions: dict[str, str],
) -> list[PackageSmokeScenario]:
    """Build isolated consumers from the temporary archive-backed Cargo source."""

    headless_direct = frozenset({"jellyflow-layout", "jellyflow-runtime"})
    headless_closure = frozenset(
        {"jellyflow-core", "jellyflow-layout", "jellyflow-runtime"}
    )
    egui_direct = frozenset({"jellyflow-egui"})
    egui_closure = frozenset(
        {"jellyflow-core", "jellyflow-egui", "jellyflow-runtime"}
    )
    open_gpui_direct = frozenset(
        {"jellyflow-core", "jellyflow-runtime", "jellyflow-open-gpui"}
    )
    open_gpui_closure = frozenset(
        {"jellyflow-core", "jellyflow-open-gpui", "jellyflow-runtime"}
    )
    egui_ecosystem = frozenset({"ecolor", "emath", "epaint"})

    return [
        PackageSmokeScenario(
            consumer_name="headless",
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
            direct_jellyflow_dependencies=headless_direct,
            direct_external_dependencies=frozenset(),
            expected_jellyflow_closure=headless_closure,
            forbidden_packages=frozenset(
                {"jellyflow-egui", "jellyflow-open-gpui"}
            )
            | egui_ecosystem,
            forbidden_package_families=("egui", "gpui", "open-gpui", "wgpu", "winit"),
        ),
        PackageSmokeScenario(
            consumer_name="egui",
            smoke=SmokeScenario(
                slug="jellyflow-package-egui-smoke",
                cargo_toml=external_cargo_toml(
                    "jellyflow-package-egui-smoke",
                    [packaged_jellyflow_dependency("jellyflow-egui", versions)],
                ),
                main_rs=_load_external_consumer_main_rs("package-egui"),
            ),
            direct_jellyflow_dependencies=egui_direct,
            direct_external_dependencies=frozenset(),
            expected_jellyflow_closure=egui_closure,
            forbidden_packages=frozenset(
                {"jellyflow-layout", "jellyflow-open-gpui"}
            ),
            forbidden_package_families=("gpui", "open-gpui"),
        ),
        PackageSmokeScenario(
            consumer_name="open-gpui",
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
            direct_jellyflow_dependencies=open_gpui_direct,
            direct_external_dependencies=frozenset(
                {"open_gpui", "open_gpui_ui_components"}
            ),
            expected_jellyflow_closure=open_gpui_closure,
            forbidden_packages=frozenset({"jellyflow-egui", "jellyflow-layout"})
            | egui_ecosystem,
            forbidden_package_families=("egui",),
        ),
    ]


def select_package_consumer_scenarios(
    scenarios: Sequence[PackageSmokeScenario],
    consumer_names: Sequence[str] | None,
) -> list[PackageSmokeScenario]:
    if consumer_names is None:
        return list(scenarios)

    selected_names = set(consumer_names)
    if not selected_names:
        raise ValueError("at least one package consumer must be selected")
    known_names = {scenario.consumer_name for scenario in scenarios}
    unknown_names = selected_names - known_names
    if unknown_names:
        raise ValueError(f"unknown package consumers: {sorted(unknown_names)}")
    return [
        scenario for scenario in scenarios if scenario.consumer_name in selected_names
    ]


def jellyflow_open_gpui_source_scenario(repo_root: Path) -> SmokeScenario:
    return SmokeScenario(
        slug="jellyflow-open-gpui-external-smoke",
        cargo_subcommand="test",
        cargo_toml=external_cargo_toml(
            "jellyflow-open-gpui-external-smoke",
            [
                f'jellyflow-core = {{ path = "{(repo_root / "crates/jellyflow-core").as_posix()}" }}',
                f'jellyflow-open-gpui = {{ path = "{(repo_root / "crates/jellyflow-open-gpui").as_posix()}" }}',
                f'jellyflow-runtime = {{ path = "{(repo_root / "crates/jellyflow-runtime").as_posix()}" }}',
                'open_gpui = { package = "open-gpui", version = "=0.2.0", default-features = false, features = ["test-support"] }',
                'open_gpui_ui_components = { package = "open-gpui-ui-components", version = "=0.2.0", default-features = false }',
            ],
        ),
        main_rs=open_gpui_adoption_main_rs(),
    )


def jellyflow_adapter_conformance_source_scenario(repo_root: Path) -> SmokeScenario:
    return SmokeScenario(
        slug="jellyflow-adapter-conformance-external-smoke",
        cargo_subcommand="test",
        cargo_toml=external_cargo_toml(
            "jellyflow-adapter-conformance-external-smoke",
            [
                f'jellyflow-core = {{ path = "{(repo_root / "crates/jellyflow-core").as_posix()}" }}',
                f'jellyflow-runtime = {{ path = "{(repo_root / "crates/jellyflow-runtime").as_posix()}", features = ["adapter-conformance"] }}',
            ],
        ),
        main_rs=_load_external_consumer_main_rs("adapter-conformance"),
    )


def make_temp_root(keep: bool) -> tuple[Path, tempfile.TemporaryDirectory[str] | None]:
    if keep:
        return Path(tempfile.mkdtemp(prefix="jellyflow_external_smoke_")), None

    context = tempfile.TemporaryDirectory(prefix="jellyflow_external_smoke_")
    return Path(context.name), context


def scenario_project_dir(
    temp_root: Path,
    scenario: SmokeScenario,
    scenario_count: int,
) -> Path:
    if scenario_count == 1:
        return temp_root
    return temp_root / scenario.slug


def write_scenario(project_dir: Path, scenario: SmokeScenario) -> None:
    write_file(project_dir / "Cargo.toml", scenario.cargo_toml)
    write_file(project_dir / "src/main.rs", scenario.main_rs)


def run_cargo_manifest(
    cwd: Path,
    manifest_path: Path,
    cargo_subcommand: str = "run",
    locked: bool = False,
) -> None:
    command = [
        "cargo",
        cargo_subcommand,
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


def cargo_tree_for_manifest(
    cwd: Path,
    manifest_path: Path,
    *,
    locked: bool = False,
) -> str:
    command = [
        "cargo",
        "tree",
        "--manifest-path",
        str(manifest_path),
        "--prefix",
        "none",
    ]
    if locked:
        command.append("--locked")
    tree = _run_cargo(
        command,
        cwd=cwd,
        capture_output=True,
    )
    return tree.stdout


def resolved_package_features(
    cwd: Path,
    manifest_path: Path,
    package_name: str,
    *,
    locked: bool = False,
) -> set[str]:
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


def forbidden_fret_packages(tree_output: str) -> list[str]:
    forbidden = []
    for line in tree_output.splitlines():
        package_name = line.strip().split(" ", 1)[0]
        if package_name == "fret" or package_name.startswith("fret-"):
            forbidden.append(line.strip())
    return forbidden


def assert_no_fret_packages(tree_output: str) -> bool:
    forbidden = forbidden_fret_packages(tree_output)
    if forbidden:
        print(
            "external Jellyflow smoke pulled Fret packages:\n" + "\n".join(forbidden),
            file=sys.stderr,
        )
        return False

    print("external cargo tree contains no fret or fret-* packages")
    return True


def run_source_path_scenario(
    repo_root: Path, project_dir: Path, scenario: SmokeScenario
) -> bool:
    write_scenario(project_dir, scenario)
    print(f"source-path interaction smoke project: {project_dir}", flush=True)
    manifest = project_dir / "Cargo.toml"
    _run_cargo(
        ["cargo", "generate-lockfile", "--manifest-path", str(manifest)],
        cwd=repo_root,
    )
    run_cargo_manifest(
        repo_root,
        manifest,
        cargo_subcommand=scenario.cargo_subcommand,
        locked=True,
    )
    return assert_no_fret_packages(
        cargo_tree_for_manifest(repo_root, manifest, locked=True)
    )


def assert_registry_only_consumer(
    manifest_path: Path,
    scenario: PackageSmokeScenario,
    versions: dict[str, str],
) -> None:
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    if "workspace" in manifest:
        raise RuntimeError("archive-backed consumer must not define a workspace")
    dependencies = manifest.get("dependencies")
    if not isinstance(dependencies, dict):
        raise RuntimeError(f"consumer dependencies missing from {manifest_path}")
    expected_dependencies = (
        scenario.direct_jellyflow_dependencies | scenario.direct_external_dependencies
    )
    if dependencies.keys() != expected_dependencies:
        raise RuntimeError(
            f"{scenario.smoke.slug} declares {sorted(dependencies)}, expected only "
            f"{sorted(expected_dependencies)}"
        )
    for name, dependency in dependencies.items():
        if isinstance(dependency, dict) and (
            "git" in dependency or "path" in dependency or "workspace" in dependency
        ):
            raise RuntimeError(
                f"archive-backed consumer dependency {name} uses a non-registry source"
            )
    for name in scenario.direct_jellyflow_dependencies:
        dependency = dependencies.get(name)
        if not isinstance(dependency, dict) or dependency.get("registry") != (
            "jellyflow-package-archives"
        ):
            raise RuntimeError(f"archive-backed dependency {name} bypasses the archive registry")
        if dependency.get("version") != f"={versions[name]}":
            raise RuntimeError(
                f"archive-backed dependency {name} does not pin candidate version "
                f"{versions[name]}"
            )
    open_gpui = dependencies.get("open_gpui")
    if open_gpui is not None:
        if not isinstance(open_gpui, dict):
            raise RuntimeError("archive-backed Open GPUI dependency must be explicit")
        if open_gpui.get("package") != "open-gpui" or open_gpui.get("version") != "=0.2.0":
            raise RuntimeError("archive-backed Open GPUI dependency identity changed")
        features = open_gpui.get("features", [])
        if isinstance(features, list) and "test-support" in features:
            raise RuntimeError("archive-backed Open GPUI consumer enabled test-support")
        if open_gpui.get("default-features") is not False:
            raise RuntimeError("archive-backed Open GPUI consumer enabled default features")


def package_names_in_tree(tree_output: str) -> set[str]:
    packages = set()
    for line in tree_output.splitlines():
        fields = line.strip().split()
        if len(fields) >= 2 and fields[1].startswith("v"):
            packages.add(fields[0])
    return packages


def assert_package_tree_boundary(
    tree_output: str, scenario: PackageSmokeScenario
) -> set[str]:
    packages = package_names_in_tree(tree_output)
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

    forbidden = packages.intersection(scenario.forbidden_packages)
    forbidden.update(
        package
        for package in packages
        for family in scenario.forbidden_package_families
        if package == family or package.startswith(f"{family}-")
    )
    if forbidden:
        raise RuntimeError(
            f"{scenario.smoke.slug} crossed its package boundary: {sorted(forbidden)}"
        )

    print(
        f"{scenario.smoke.slug} resolves exact Jellyflow closure: "
        + ", ".join(sorted(jellyflow_packages))
    )
    return jellyflow_packages


def assert_package_lock_sources(
    lock_path: Path,
    index_url: str,
    packaged: Iterable[PackagedCrate],
    expected_names: frozenset[str],
    consumer_name: str,
) -> None:
    lock = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    entries = lock.get("package", [])
    packaged_by_name = {package.name: package for package in packaged}
    expected_packages = [packaged_by_name[name] for name in sorted(expected_names)]
    candidate_identities = {
        (package.name, package.version) for package in packaged_by_name.values()
    }
    allowed_jellyflow_packages = set(PUBLISHABLE_CRATES) | {consumer_name}
    unexpected_jellyflow_packages = sorted(
        {
            entry["name"]
            for entry in entries
            if is_jellyflow_release_namespace(entry["name"])
            and entry["name"] not in allowed_jellyflow_packages
        }
    )
    if unexpected_jellyflow_packages:
        raise RuntimeError(
            "lockfile pulled Jellyflow packages outside the release set: "
            f"{unexpected_jellyflow_packages}"
        )
    unexpected = [
        (entry["name"], entry["version"], entry.get("source"))
        for entry in entries
        if entry["name"] in PUBLISHABLE_CRATES
        and (entry["name"], entry["version"]) not in candidate_identities
    ]
    if unexpected:
        raise RuntimeError(f"lockfile pulled non-candidate Jellyflow packages: {unexpected}")
    resolved_names = {
        entry["name"] for entry in entries if entry["name"] in PUBLISHABLE_CRATES
    }
    if resolved_names != expected_names:
        raise RuntimeError(
            f"lockfile resolved Jellyflow packages {sorted(resolved_names)}, expected "
            f"{sorted(expected_names)}"
        )
    expected_source = index_url
    for package in expected_packages:
        matches = [
            entry
            for entry in entries
            if entry["name"] == package.name and entry["version"] == package.version
        ]
        if len(matches) != 1:
            raise RuntimeError(
                f"lockfile contains {len(matches)} sources for "
                f"{package.name} {package.version}"
            )
        entry = matches[0]
        if entry.get("source") != expected_source:
            raise RuntimeError(
                f"{package.name} resolved from {entry.get('source')}, expected {expected_source}"
            )
        if entry.get("checksum") != sha256_file(package.archive):
            raise RuntimeError(f"lockfile checksum does not match {package.archive}")


def run_package_consumer_smoke(
    project_dir: Path,
    index_url: str,
    packaged: list[PackagedCrate],
    scenario: PackageSmokeScenario,
    target_dir: Path,
) -> set[str]:
    write_scenario(project_dir, scenario.smoke)
    write_file(
        project_dir / ".cargo/config.toml",
        package_registry_config(index_url, target_dir),
    )
    manifest = project_dir / "Cargo.toml"
    versions = {package.name: package.version for package in packaged}
    assert_registry_only_consumer(manifest, scenario, versions)
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
    assert_package_lock_sources(
        project_dir / "Cargo.lock",
        index_url,
        packaged,
        scenario.expected_jellyflow_closure,
        scenario.smoke.slug,
    )
    tree = cargo_tree_for_manifest(project_dir, manifest, locked=True)
    jellyflow_closure = assert_package_tree_boundary(tree, scenario)
    if "open_gpui" in scenario.direct_external_dependencies:
        open_gpui_features = resolved_package_features(
            project_dir, manifest, "open-gpui", locked=True
        )
        for feature in ("default", "test-support"):
            if feature in open_gpui_features:
                raise RuntimeError(
                    f"archive-backed Open GPUI consumer activated {feature}"
                )
        print(
            "archive-backed Open GPUI consumer excludes default and test-support features"
        )
    if "jellyflow-runtime" in scenario.expected_jellyflow_closure:
        runtime_features = resolved_package_features(
            project_dir, manifest, "jellyflow-runtime", locked=True
        )
        if "adapter-conformance" in runtime_features:
            raise RuntimeError(
                "archive-backed Jellyflow consumer activated adapter-conformance"
            )
        print("archive-backed Jellyflow consumer excludes adapter-conformance")
    if not assert_no_fret_packages(tree):
        raise RuntimeError(f"{scenario.smoke.slug} resolved forbidden Fret packages")
    run_cargo_manifest(project_dir, manifest, locked=True)
    print(f"{scenario.smoke.slug} compiled from candidate archives")
    return jellyflow_closure


def run_package_consumers_smoke(
    project_root: Path,
    index_url: str,
    packaged: list[PackagedCrate],
    consumer_names: Sequence[str] | None = None,
) -> bool:
    versions = {package.name: package.version for package in packaged}
    scenarios = select_package_consumer_scenarios(
        jellyflow_package_consumer_scenarios(versions), consumer_names
    )
    resolved_union = set()
    target_dir = project_root / "target"
    for scenario in scenarios:
        resolved_union.update(
            run_package_consumer_smoke(
                project_root / scenario.smoke.slug,
                index_url,
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
    if expected_union == set(PUBLISHABLE_CRATES):
        print("isolated package consumers cover the complete candidate archive set")
    else:
        print(
            "isolated package consumers cover the selected candidate archive closure: "
            + ", ".join(sorted(expected_union))
        )
    return True


def parse_arguments(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Create temporary Cargo projects outside this workspace and exercise the public "
            "Jellyflow runtime and native adapter packages."
        )
    )
    parser.add_argument(
        "--keep",
        action="store_true",
        help="keep the temporary smoke projects for inspection",
    )
    parser.add_argument(
        "--packages-only",
        action="store_true",
        help="run archive-backed package consumers without source-path consumers",
    )
    parser.add_argument(
        "--package-consumer",
        action="append",
        choices=PACKAGE_CONSUMER_NAMES,
        help=(
            "run only the named archive-backed consumer; repeat to select multiple "
            "consumers (requires --packages-only)"
        ),
    )
    args = parser.parse_args(argv)
    if args.package_consumer and not args.packages_only:
        parser.error("--package-consumer requires --packages-only")
    return args


def source_path_scenarios_for_run(
    repo_root: Path, *, packages_only: bool
) -> list[SmokeScenario]:
    if packages_only:
        return []
    return jellyflow_source_path_smoke_scenarios(repo_root)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_arguments(argv)

    repo_root = ROOT
    source_scenarios = source_path_scenarios_for_run(
        repo_root, packages_only=args.packages_only
    )
    temp_root, temp_context = make_temp_root(args.keep)

    try:
        package_root = temp_root / "package-source"
        packaged = assemble_package_archives(repo_root, package_root)
        metadata = resolve_package_metadata(packaged)
        with serve_sparse_registry(
            package_root / "registry", packaged, metadata
        ) as index_url:
            ok = run_package_consumers_smoke(
                temp_root / "package-consumers",
                index_url,
                packaged,
                args.package_consumer,
            )
        source_root = temp_root / "source-path-consumers"
        for scenario in source_scenarios:
            project_dir = scenario_project_dir(
                source_root, scenario, len(source_scenarios)
            )
            ok = run_source_path_scenario(repo_root, project_dir, scenario) and ok
        return 0 if ok else 1
    finally:
        if args.keep:
            print(f"kept external smoke project: {temp_root}")
        elif temp_context is not None:
            temp_context.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())
