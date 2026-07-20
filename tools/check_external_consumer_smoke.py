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
    print("candidate archives assembled; package-derived compile verification follows")
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


def package_registry_config(index_url: str) -> str:
    return textwrap.dedent(
        f"""
        [registries.jellyflow-package-archives]
        index = {json.dumps(index_url)}
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


def jellyflow_package_consumer_scenario(versions: dict[str, str]) -> SmokeScenario:
    """Build one consumer from the temporary package-derived Cargo source."""

    dependencies = [
        f'{name} = {{ version = "={versions[name]}", '
        'registry = "jellyflow-package-archives" }'
        for name in PUBLISHABLE_CRATES
    ]
    dependencies.append(
        'open_gpui = { package = "open-gpui", version = "=0.2.0", '
        "default-features = false }"
    )
    dependencies.append('serde_json = "1"')
    return SmokeScenario(
        slug="jellyflow-package-derived-external-smoke",
        cargo_toml=external_cargo_toml(
            "jellyflow-package-derived-external-smoke", dependencies
        ),
        main_rs=_load_external_consumer_main_rs("package-derived"),
    )


def jellyflow_open_gpui_source_scenario(repo_root: Path) -> SmokeScenario:
    return SmokeScenario(
        slug="jellyflow-open-gpui-external-smoke",
        cargo_subcommand="test",
        cargo_toml=external_cargo_toml(
            "jellyflow-open-gpui-external-smoke",
            [
                f'jellyflow = {{ path = "{(repo_root / "crates/jellyflow").as_posix()}" }}',
                f'jellyflow-open-gpui = {{ path = "{(repo_root / "crates/jellyflow-open-gpui").as_posix()}" }}',
                'open_gpui = { package = "open-gpui", version = "=0.2.0", default-features = false, features = ["test-support"] }',
            ],
        ),
        main_rs=_load_external_consumer_main_rs("open-gpui-source"),
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


def assert_registry_only_consumer(manifest_path: Path) -> None:
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    if "workspace" in manifest:
        raise RuntimeError("package-derived consumer must not define a workspace")
    dependencies = manifest.get("dependencies")
    if not isinstance(dependencies, dict):
        raise RuntimeError(f"consumer dependencies missing from {manifest_path}")
    for name, dependency in dependencies.items():
        if isinstance(dependency, dict) and (
            "git" in dependency or "path" in dependency or "workspace" in dependency
        ):
            raise RuntimeError(
                f"package-derived consumer dependency {name} uses a non-registry source"
            )
    for name in PUBLISHABLE_CRATES:
        dependency = dependencies.get(name)
        if not isinstance(dependency, dict) or dependency.get("registry") != (
            "jellyflow-package-archives"
        ):
            raise RuntimeError(f"package-derived dependency {name} bypasses the archive registry")
    open_gpui = dependencies.get("open_gpui")
    if not isinstance(open_gpui, dict):
        raise RuntimeError("package-derived consumer must declare Open GPUI explicitly")
    features = open_gpui.get("features", [])
    if isinstance(features, list) and "test-support" in features:
        raise RuntimeError("package-derived Open GPUI consumer enabled test-support")
    if open_gpui.get("default-features") is not False:
        raise RuntimeError("package-derived Open GPUI consumer enabled default features")


def assert_packaged_crates_in_tree(
    tree_output: str, packaged: Iterable[PackagedCrate]
) -> bool:
    missing = [
        f"{package.name} v{package.version}"
        for package in packaged
        if f"{package.name} v{package.version}" not in tree_output
    ]
    if missing:
        print(
            "package-derived consumer omitted packaged crates:\n" + "\n".join(missing),
            file=sys.stderr,
        )
        return False
    print("external cargo tree resolves all Jellyflow package archives")
    return True


def assert_package_lock_sources(
    lock_path: Path, index_url: str, packaged: Iterable[PackagedCrate]
) -> None:
    lock = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    entries = lock.get("package", [])
    expected_identities = {(package.name, package.version) for package in packaged}
    unexpected = [
        (entry["name"], entry["version"], entry.get("source"))
        for entry in entries
        if entry["name"] in PUBLISHABLE_CRATES
        and (entry["name"], entry["version"]) not in expected_identities
    ]
    if unexpected:
        raise RuntimeError(f"lockfile pulled non-candidate Jellyflow packages: {unexpected}")
    expected_source = index_url
    for package in packaged:
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
) -> bool:
    versions = {package.name: package.version for package in packaged}
    scenario = jellyflow_package_consumer_scenario(versions)
    write_scenario(project_dir, scenario)
    write_file(project_dir / ".cargo/config.toml", package_registry_config(index_url))
    manifest = project_dir / "Cargo.toml"
    assert_registry_only_consumer(manifest)
    print(f"package-derived external smoke project: {project_dir}", flush=True)
    _run_cargo(
        [
            "cargo",
            "generate-lockfile",
            "--manifest-path",
            str(manifest),
        ],
        cwd=project_dir,
    )
    assert_package_lock_sources(project_dir / "Cargo.lock", index_url, packaged)
    tree = cargo_tree_for_manifest(project_dir, manifest, locked=True)
    open_gpui_features = resolved_package_features(
        project_dir, manifest, "open-gpui", locked=True
    )
    for feature in ("default", "test-support"):
        if feature in open_gpui_features:
            print(
                f"package-derived Open GPUI consumer activated {feature}",
                file=sys.stderr,
            )
            return False
    print("package-derived Open GPUI consumer excludes default and test-support features")
    runtime_features = resolved_package_features(
        project_dir, manifest, "jellyflow-runtime", locked=True
    )
    if "adapter-conformance" in runtime_features:
        print(
            "package-derived Jellyflow consumer activated adapter-conformance",
            file=sys.stderr,
        )
        return False
    print("package-derived Jellyflow consumer excludes adapter-conformance")
    if not assert_packaged_crates_in_tree(tree, packaged) or not assert_no_fret_packages(
        tree
    ):
        return False
    run_cargo_manifest(project_dir, manifest, locked=True)
    print("package-derived external consumer compiled from candidate archives")
    return True


def run_template_adapter_smoke(repo_root: Path) -> bool:
    template_manifest = repo_root / "templates/native-adapter/Cargo.toml"
    print(f"native adapter template smoke: {template_manifest.parent}", flush=True)
    run_cargo_manifest(
        repo_root,
        template_manifest,
        cargo_subcommand="test",
        locked=True,
    )
    print("native adapter template protocol test passed")
    return assert_no_fret_packages(
        cargo_tree_for_manifest(repo_root, template_manifest, locked=True)
    )


def main() -> int:
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
    args = parser.parse_args()

    repo_root = ROOT
    scenarios = jellyflow_source_path_smoke_scenarios(repo_root)
    temp_root, temp_context = make_temp_root(args.keep)

    try:
        package_root = temp_root / "package-source"
        packaged = assemble_package_archives(repo_root, package_root)
        metadata = resolve_package_metadata(packaged)
        with serve_sparse_registry(
            package_root / "registry", packaged, metadata
        ) as index_url:
            ok = run_package_consumer_smoke(
                temp_root / "package-consumer",
                index_url,
                packaged,
            )
        source_root = temp_root / "source-path-consumers"
        for scenario in scenarios:
            project_dir = scenario_project_dir(source_root, scenario, len(scenarios))
            ok = run_source_path_scenario(repo_root, project_dir, scenario) and ok
        ok = run_template_adapter_smoke(repo_root) and ok
        return 0 if ok else 1
    finally:
        if args.keep:
            print(f"kept external smoke project: {temp_root}")
        elif temp_context is not None:
            temp_context.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())
