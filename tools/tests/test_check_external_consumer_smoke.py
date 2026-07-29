from __future__ import annotations

import errno
import importlib.util
import io
import os
import signal
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "check_external_consumer_smoke.py"
FIXTURE_ROOT = SCRIPT_PATH.parent / "fixtures" / "external-consumer"
SPEC = importlib.util.spec_from_file_location("check_external_consumer_smoke", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {SCRIPT_PATH}")
external_smoke = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = external_smoke
SPEC.loader.exec_module(external_smoke)


class ExternalConsumerScenarioTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "posix", "POSIX process groups only")
    def test_cleanup_eperm_does_not_replace_the_timeout_error(self) -> None:
        attempted_signals: list[int] = []

        def deny_process_group_signal(
            process_group_id: int, signal_number: int
        ) -> None:
            attempted_signals.append(signal_number)
            raise PermissionError(
                errno.EPERM,
                f"cannot signal process group {process_group_id}",
            )

        with (
            patch.object(os, "killpg", side_effect=deny_process_group_signal),
            self.assertRaises(subprocess.TimeoutExpired) as raised,
        ):
            external_smoke.run_process(
                [sys.executable, "-c", "import time; time.sleep(30)"],
                timeout_seconds=0.05,
                kill_grace_seconds=0.01,
            )

        self.assertEqual(attempted_signals, [signal.SIGTERM])
        self.assertTrue(
            any(
                "process-group cleanup failed" in note
                for note in raised.exception.__notes__
            )
        )

    @unittest.skipUnless(os.name == "posix", "POSIX process groups only")
    def test_cargo_timeout_survives_probe_eperm_and_terminates_descendants(self) -> None:
        descendant_pid: int | None = None
        original_killpg = os.killpg
        probe_permission_denied = False

        def killpg_with_transient_probe_eperm(
            process_group_id: int, signal_number: int
        ) -> None:
            nonlocal probe_permission_denied
            if signal_number == 0 and not probe_permission_denied:
                probe_permission_denied = True
                raise PermissionError(errno.EPERM, os.strerror(errno.EPERM))
            original_killpg(process_group_id, signal_number)

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bin_dir = root / "bin"
            bin_dir.mkdir()
            pid_file = root / "descendant.pid"
            manifest = root / "Cargo.toml"
            manifest.touch()
            fake_cargo = bin_dir / "cargo"
            fake_cargo.write_text(
                "\n".join(
                    [
                        f"#!{sys.executable}",
                        "import os",
                        "import subprocess",
                        "import sys",
                        "import time",
                        (
                            "child = subprocess.Popen("
                            "[sys.executable, '-c', 'import time; time.sleep(30)'])"
                        ),
                        (
                            "with open(os.environ['FAKE_CARGO_DESCENDANT_PID'], "
                            "'w', encoding='utf-8') as file:"
                        ),
                        "    file.write(str(child.pid))",
                        "time.sleep(30)",
                        "",
                    ]
                ),
                encoding="utf-8",
            )
            fake_cargo.chmod(0o755)
            environment = {
                "FAKE_CARGO_DESCENDANT_PID": str(pid_file),
                "PATH": f"{bin_dir}{os.pathsep}{os.environ.get('PATH', '')}",
            }

            try:
                with (
                    patch.dict(os.environ, environment),
                    patch.object(
                        os,
                        "killpg",
                        side_effect=killpg_with_transient_probe_eperm,
                    ),
                    patch.object(
                        external_smoke,
                        "CARGO_COMMAND_TIMEOUT_SECONDS",
                        0.5,
                    ),
                    self.assertRaises(subprocess.TimeoutExpired),
                ):
                    external_smoke.run_cargo_manifest(root, manifest)

                self.assertTrue(probe_permission_denied)
                descendant_pid = int(pid_file.read_text(encoding="utf-8"))
                with self.assertRaises(ProcessLookupError):
                    os.kill(descendant_pid, 0)
            finally:
                if descendant_pid is None and pid_file.exists():
                    descendant_pid = int(pid_file.read_text(encoding="utf-8"))
                if descendant_pid is not None:
                    try:
                        os.kill(descendant_pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass

    def test_fixture_loader_reads_checked_in_consumer_sources(self) -> None:
        for fixture_name in (
            "adapter-conformance",
            "open-gpui-adoption",
            "package-egui",
            "package-headless",
        ):
            with self.subTest(fixture_name=fixture_name):
                expected = (FIXTURE_ROOT / fixture_name / "src/main.rs").read_text(
                    encoding="utf-8"
                )

                self.assertEqual(
                    external_smoke._load_external_consumer_main_rs(fixture_name),
                    expected,
                )

    def test_open_gpui_source_and_archive_share_one_production_recipe(self) -> None:
        versions = {name: "0.0.0-test" for name in external_smoke.PUBLISHABLE_CRATES}
        package_scenarios = {
            scenario.consumer_name: scenario
            for scenario in external_smoke.jellyflow_package_consumer_scenarios(versions)
        }

        shared_recipe = external_smoke.open_gpui_adoption_main_rs()
        self.assertIn("fn build_product_registry", shared_recipe)
        self.assertIn("OpenGpuiNodeRendererDescriptor::new", shared_recipe)
        self.assertIn("OpenGpuiNodeAdapterPresentation::new", shared_recipe)
        self.assertIn("RendererStamp::new", shared_recipe)
        self.assertIn(".seal(catalog)", shared_recipe)
        self.assertIn("fn assert_invalid_registry_is_rejected", shared_recipe)
        self.assertIn("fn main() {", shared_recipe)
        self.assertIn("let registry = build_product_registry", shared_recipe)
        self.assertIn("assert_invalid_registry_is_rejected(&catalog);", shared_recipe)
        self.assertIn("TextInput::new", shared_recipe)
        self.assertIn("Select::new", shared_recipe)
        self.assertIn("Button::new", shared_recipe)
        self.assertIn("overflow_y_scroll", shared_recipe)
        self.assertIn("OpenGpuiManagedMutation", shared_recipe)
        self.assertIn("managed_mutation", shared_recipe)
        self.assertNotIn("OpenGpuiCommandTicket", shared_recipe)
        self.assertNotIn(".enqueue(", shared_recipe)
        self.assertNotIn("OpenGpuiActivityKind::Task", shared_recipe)
        self.assertNotIn("command_task", shared_recipe)
        self.assertNotIn("invalidate_layout(", shared_recipe)
        self.assertNotIn("register_with_policy", shared_recipe)
        self.assertNotIn("register_with_overview", shared_recipe)
        self.assertNotIn("try_register", shared_recipe)
        self.assertNotIn("OpenGpuiRendererRegistrationStatus", shared_recipe)
        self.assertEqual(
            external_smoke.jellyflow_open_gpui_source_scenario(
                Path("/tmp/source")
            ).main_rs,
            shared_recipe,
        )
        archive = package_scenarios["open-gpui"]
        self.assertEqual(archive.smoke.main_rs, shared_recipe)
        self.assertEqual(
            archive.direct_jellyflow_dependencies,
            {"jellyflow-core", "jellyflow-runtime", "jellyflow-open-gpui"},
        )
        self.assertEqual(
            archive.direct_external_dependencies,
            {"open_gpui", "open_gpui_ui_components"},
        )
        self.assertEqual(
            archive.expected_jellyflow_closure,
            {"jellyflow-core", "jellyflow-runtime", "jellyflow-open-gpui"},
        )
        self.assertNotIn("test-support", archive.smoke.cargo_toml)
        source = external_smoke.jellyflow_open_gpui_source_scenario(Path("/tmp/source"))
        self.assertIn("open_gpui_ui_components", source.cargo_toml)

    def test_normalized_manifest_rejects_unpublished_jellyflow_dependencies(self) -> None:
        for package in ("jellyflow", "jellyflow-unknown"):
            with self.subTest(package=package), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "Cargo.toml").write_text(
                    "\n".join(
                        [
                            '[package]',
                            'name = "fixture"',
                            'version = "0.0.0"',
                            '',
                            '[dependencies]',
                            f'unknown = {{ package = "{package}", version = "0.4" }}',
                            '',
                        ]
                    ),
                    encoding="utf-8",
                )

                with self.assertRaisesRegex(RuntimeError, "outside the release set"):
                    external_smoke.normalized_package_manifest(root)

    def test_import_and_scenario_invocation_are_independent_of_cwd(self) -> None:
        probe = "\n".join(
            [
                "import importlib.util",
                "import pathlib",
                "import sys",
                f"script_path = pathlib.Path({str(SCRIPT_PATH)!r})",
                "spec = importlib.util.spec_from_file_location('cwd_external_smoke', script_path)",
                "assert spec is not None and spec.loader is not None",
                "module = importlib.util.module_from_spec(spec)",
                "sys.modules[spec.name] = module",
                "spec.loader.exec_module(module)",
                "versions = {name: '0.0.0-test' for name in module.PUBLISHABLE_CRATES}",
                (
                    "package_scenarios = {scenario.smoke.slug: scenario.smoke "
                    "for scenario in module.jellyflow_package_consumer_scenarios(versions)}"
                ),
                (
                    "assert package_scenarios['jellyflow-package-headless-smoke'].main_rs "
                    "== module._load_external_consumer_main_rs('package-headless')"
                ),
                (
                    "assert package_scenarios['jellyflow-package-egui-smoke'].main_rs "
                    "== module._load_external_consumer_main_rs('package-egui')"
                ),
                (
                    "assert package_scenarios['jellyflow-package-open-gpui-smoke'].main_rs "
                    "== module.open_gpui_adoption_main_rs()"
                ),
                (
                    "assert module.jellyflow_open_gpui_source_scenario("
                    "pathlib.Path('/tmp/source')).main_rs "
                    "== module.open_gpui_adoption_main_rs()"
                ),
                (
                    "assert module.jellyflow_adapter_conformance_source_scenario("
                    "pathlib.Path('/tmp/source')).main_rs "
                    "== module._load_external_consumer_main_rs('adapter-conformance')"
                ),
            ]
        )
        with tempfile.TemporaryDirectory() as directory:
            subprocess.run(
                [sys.executable, "-c", probe],
                cwd=directory,
                check=True,
                capture_output=True,
                text=True,
            )

    def test_package_extraction_rejects_links_and_special_members(self) -> None:
        for member_type in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.FIFOTYPE):
            with self.subTest(member_type=member_type), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                archive = root / "unsafe.crate"
                with tarfile.open(archive, mode="w:gz") as package:
                    manifest = tarfile.TarInfo("fixture-0.0.0/Cargo.toml")
                    contents = b"[package]\nname = \"fixture\"\nversion = \"0.0.0\"\n"
                    manifest.size = len(contents)
                    package.addfile(manifest, io.BytesIO(contents))
                    unsafe = tarfile.TarInfo("fixture-0.0.0/unsafe")
                    unsafe.type = member_type
                    unsafe.linkname = "Cargo.toml"
                    package.addfile(unsafe)

                with self.assertRaisesRegex(RuntimeError, "unsafe member type"):
                    external_smoke.extract_package_archive(archive, root / "out")

    def test_package_scenarios_isolate_each_published_boundary(self) -> None:
        versions = {name: "0.0.0-test" for name in external_smoke.PUBLISHABLE_CRATES}
        scenarios = {
            scenario.smoke.slug: scenario
            for scenario in external_smoke.jellyflow_package_consumer_scenarios(
                versions
            )
        }
        self.assertEqual(
            set(scenarios),
            {
                "jellyflow-package-egui-smoke",
                "jellyflow-package-headless-smoke",
                "jellyflow-package-open-gpui-smoke",
            },
        )
        expected_direct = {
            "jellyflow-package-egui-smoke": {"jellyflow-egui"},
            "jellyflow-package-headless-smoke": {
                "jellyflow-layout",
                "jellyflow-runtime",
            },
            "jellyflow-package-open-gpui-smoke": {
                "jellyflow-core",
                "jellyflow-runtime",
                "jellyflow-open-gpui",
                "open_gpui",
                "open_gpui_ui_components",
            },
        }
        expected_closures = {
            "jellyflow-package-egui-smoke": {
                "jellyflow-core",
                "jellyflow-egui",
                "jellyflow-runtime",
            },
            "jellyflow-package-headless-smoke": {
                "jellyflow-core",
                "jellyflow-layout",
                "jellyflow-runtime",
            },
            "jellyflow-package-open-gpui-smoke": {
                "jellyflow-core",
                "jellyflow-open-gpui",
                "jellyflow-runtime",
            },
        }
        for slug, scenario in scenarios.items():
            with self.subTest(slug=slug):
                dependencies = tomllib.loads(scenario.smoke.cargo_toml)[
                    "dependencies"
                ]
                self.assertEqual(set(dependencies), expected_direct[slug])
                self.assertEqual(
                    set(scenario.expected_jellyflow_closure), expected_closures[slug]
                )
                for name in scenario.direct_jellyflow_dependencies:
                    self.assertEqual(
                        dependencies[name],
                        {
                            "version": "=0.0.0-test",
                            "registry": "jellyflow-package-archives",
                        },
                    )

        dependencies = tomllib.loads(
            scenarios["jellyflow-package-open-gpui-smoke"].smoke.cargo_toml
        )["dependencies"]
        self.assertEqual(
            dependencies["open_gpui"],
            {
                "package": "open-gpui",
                "version": "=0.2.0",
                "default-features": False,
            },
        )

    def test_default_cli_runs_all_package_and_source_path_consumers(self) -> None:
        args = external_smoke.parse_arguments([])
        versions = {
            name: "0.0.0-test" for name in external_smoke.PUBLISHABLE_CRATES
        }
        package_scenarios = external_smoke.select_package_consumer_scenarios(
            external_smoke.jellyflow_package_consumer_scenarios(versions),
            args.package_consumer,
        )
        source_scenarios = external_smoke.source_path_scenarios_for_run(
            Path("/tmp/jellyflow-source"), packages_only=args.packages_only
        )

        self.assertFalse(args.packages_only)
        self.assertEqual(
            {scenario.consumer_name for scenario in package_scenarios},
            set(external_smoke.PACKAGE_CONSUMER_NAMES),
        )
        self.assertEqual(len(source_scenarios), 2)

    def test_packages_only_can_select_the_open_gpui_archive_consumer(self) -> None:
        args = external_smoke.parse_arguments(
            ["--packages-only", "--package-consumer", "open-gpui"]
        )
        versions = {
            name: "0.0.0-test" for name in external_smoke.PUBLISHABLE_CRATES
        }
        scenarios = external_smoke.select_package_consumer_scenarios(
            external_smoke.jellyflow_package_consumer_scenarios(versions),
            args.package_consumer,
        )

        self.assertEqual(
            [(scenario.consumer_name, scenario.smoke.slug) for scenario in scenarios],
            [("open-gpui", "jellyflow-package-open-gpui-smoke")],
        )
        self.assertEqual(
            external_smoke.source_path_scenarios_for_run(
                Path("/tmp/jellyflow-source"), packages_only=args.packages_only
            ),
            [],
        )

    def test_packages_only_without_selection_keeps_all_package_consumers(self) -> None:
        args = external_smoke.parse_arguments(["--packages-only"])
        versions = {
            name: "0.0.0-test" for name in external_smoke.PUBLISHABLE_CRATES
        }
        scenarios = external_smoke.select_package_consumer_scenarios(
            external_smoke.jellyflow_package_consumer_scenarios(versions),
            args.package_consumer,
        )

        self.assertEqual(
            {scenario.consumer_name for scenario in scenarios},
            set(external_smoke.PACKAGE_CONSUMER_NAMES),
        )

    def test_selected_package_run_uses_the_selected_archive_closure(self) -> None:
        packaged = [
            external_smoke.PackagedCrate(
                name=name,
                version="0.0.0-test",
                archive=Path("/tmp") / f"{name}.crate",
                source_dir=Path("/tmp") / name,
            )
            for name in external_smoke.PUBLISHABLE_CRATES
        ]

        def run_selected(
            project_dir: Path,
            index_url: str,
            candidate_archives: list[external_smoke.PackagedCrate],
            scenario: external_smoke.PackageSmokeScenario,
            target_dir: Path,
        ) -> set[str]:
            self.assertEqual(scenario.consumer_name, "open-gpui")
            return set(scenario.expected_jellyflow_closure)

        with patch.object(
            external_smoke,
            "run_package_consumer_smoke",
            side_effect=run_selected,
        ) as run_consumer:
            self.assertTrue(
                external_smoke.run_package_consumers_smoke(
                    Path("/tmp/package-consumers"),
                    "sparse+http://127.0.0.1:31415/index/",
                    packaged,
                    ["open-gpui"],
                )
            )

        run_consumer.assert_called_once()

    def test_package_consumer_selection_requires_packages_only(self) -> None:
        with (
            patch.object(sys, "stderr", new=io.StringIO()),
            self.assertRaises(SystemExit),
        ):
            external_smoke.parse_arguments(
                ["--package-consumer", "open-gpui"]
            )

    def test_cli_rejects_unknown_package_consumer(self) -> None:
        with (
            patch.object(sys, "stderr", new=io.StringIO()),
            self.assertRaises(SystemExit),
        ):
            external_smoke.parse_arguments(
                ["--packages-only", "--package-consumer", "unknown"]
            )

    def test_package_tree_boundary_rejects_missing_or_cross_framework_crates(self) -> None:
        versions = {name: "0.0.0-test" for name in external_smoke.PUBLISHABLE_CRATES}
        scenarios = {
            scenario.smoke.slug: scenario
            for scenario in external_smoke.jellyflow_package_consumer_scenarios(
                versions
            )
        }
        open_gpui = scenarios["jellyflow-package-open-gpui-smoke"]
        valid_tree = "\n".join(
            [
                "jellyflow-package-open-gpui-smoke v0.0.0",
                "jellyflow-open-gpui v0.0.0-test",
                "jellyflow-runtime v0.0.0-test",
                "jellyflow-core v0.0.0-test",
                "open-gpui v0.2.0",
            ]
        )
        self.assertEqual(
            external_smoke.assert_package_tree_boundary(valid_tree, open_gpui),
            set(open_gpui.expected_jellyflow_closure),
        )
        with self.assertRaisesRegex(RuntimeError, "expected"):
            external_smoke.assert_package_tree_boundary(
                valid_tree.replace("jellyflow-runtime v0.0.0-test\n", ""),
                open_gpui,
            )
        with self.assertRaisesRegex(RuntimeError, "crossed"):
            external_smoke.assert_package_tree_boundary(
                valid_tree + "\negui v0.34.3\n",
                open_gpui,
            )
        with self.assertRaisesRegex(RuntimeError, "outside the release set"):
            external_smoke.assert_package_tree_boundary(
                valid_tree + "\njellyflow-unknown v0.4.0\n",
                open_gpui,
            )
        with self.assertRaisesRegex(RuntimeError, "outside the release set"):
            external_smoke.assert_package_tree_boundary(
                valid_tree + "\njellyflow v0.4.0\n",
                open_gpui,
            )

        headless = scenarios["jellyflow-package-headless-smoke"]
        headless_tree = "\n".join(
            [
                "jellyflow-package-headless-smoke v0.0.0",
                "jellyflow-layout v0.0.0-test",
                "jellyflow-runtime v0.0.0-test",
                "jellyflow-core v0.0.0-test",
            ]
        )
        with self.assertRaisesRegex(RuntimeError, "wgpu"):
            external_smoke.assert_package_tree_boundary(
                headless_tree + "\nwgpu v28.0.0\n",
                headless,
            )

        egui = scenarios["jellyflow-package-egui-smoke"]
        egui_tree = "\n".join(
            [
                "jellyflow-package-egui-smoke v0.0.0",
                "jellyflow-egui v0.0.0-test",
                "jellyflow-runtime v0.0.0-test",
                "jellyflow-core v0.0.0-test",
            ]
        )
        with self.assertRaisesRegex(RuntimeError, "open-gpui"):
            external_smoke.assert_package_tree_boundary(
                egui_tree + "\nopen-gpui v0.2.0\n",
                egui,
            )

    @staticmethod
    def _package_lock_fixture(root: Path) -> list[external_smoke.PackagedCrate]:
        packaged = []
        for name in external_smoke.PUBLISHABLE_CRATES:
            archive = root / f"{name}.crate"
            archive.write_bytes(name.encode("utf-8"))
            packaged.append(
                external_smoke.PackagedCrate(
                    name=name,
                    version="0.0.0-test",
                    archive=archive,
                    source_dir=root / name,
                )
            )
        return packaged

    @staticmethod
    def _package_lock_text(
        packaged: list[external_smoke.PackagedCrate],
        names: set[str],
        index_url: str,
        *,
        source_overrides: dict[str, str] | None = None,
        checksum_overrides: dict[str, str] | None = None,
        version_overrides: dict[str, str] | None = None,
        duplicate_names: tuple[str, ...] = (),
    ) -> str:
        packaged_by_name = {package.name: package for package in packaged}
        source_overrides = source_overrides or {}
        checksum_overrides = checksum_overrides or {}
        version_overrides = version_overrides or {}

        def package_entry(name: str) -> list[str]:
            package = packaged_by_name[name]
            checksum = checksum_overrides.get(
                name, external_smoke.sha256_file(package.archive)
            )
            return [
                "",
                "[[package]]",
                f'name = "{name}"',
                f'version = "{version_overrides.get(name, package.version)}"',
                f'source = "{source_overrides.get(name, index_url)}"',
                f'checksum = "{checksum}"',
            ]

        entries = ["version = 4"]
        for name in sorted(names):
            entries.extend(package_entry(name))
        for name in duplicate_names:
            entries.extend(package_entry(name))
        return "\n".join(entries) + "\n"

    def _assert_package_lock_mutation_rejected(
        self,
        error: str,
        *,
        source_overrides: dict[str, str] | None = None,
        checksum_overrides: dict[str, str] | None = None,
        version_overrides: dict[str, str] | None = None,
        duplicate_names: tuple[str, ...] = (),
    ) -> None:
        index_url = "sparse+http://127.0.0.1:31415/index/"
        expected = frozenset({"jellyflow-core", "jellyflow-runtime"})
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            packaged = self._package_lock_fixture(root)
            lock_path = root / "Cargo.lock"
            lock_path.write_text(
                self._package_lock_text(
                    packaged,
                    set(expected),
                    index_url,
                    source_overrides=source_overrides,
                    checksum_overrides=checksum_overrides,
                    version_overrides=version_overrides,
                    duplicate_names=duplicate_names,
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(RuntimeError, error):
                external_smoke.assert_package_lock_sources(
                    lock_path,
                    index_url,
                    packaged,
                    expected,
                    "fixture-consumer",
                )

    def test_package_lock_sources_require_the_exact_scenario_closure(self) -> None:
        index_url = "sparse+http://127.0.0.1:31415/index/"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            packaged = self._package_lock_fixture(root)
            expected = frozenset({"jellyflow-core", "jellyflow-runtime"})
            lock_path = root / "Cargo.lock"
            lock_path.write_text(
                self._package_lock_text(packaged, set(expected), index_url),
                encoding="utf-8",
            )
            external_smoke.assert_package_lock_sources(
                lock_path,
                index_url,
                packaged,
                expected,
                "fixture-consumer",
            )

            lock_path.write_text(
                self._package_lock_text(
                    packaged,
                    set(expected) | {"jellyflow-layout"},
                    index_url,
                ),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(RuntimeError, "resolved Jellyflow packages"):
                external_smoke.assert_package_lock_sources(
                    lock_path,
                    index_url,
                    packaged,
                    expected,
                    "fixture-consumer",
                )

            lock_path.write_text(
                self._package_lock_text(packaged, set(expected), index_url)
                + '\n[[package]]\nname = "jellyflow-unknown"\nversion = "0.4.0"\n',
                encoding="utf-8",
            )
            with self.assertRaisesRegex(RuntimeError, "outside the release set"):
                external_smoke.assert_package_lock_sources(
                    lock_path,
                    index_url,
                    packaged,
                    expected,
                    "fixture-consumer",
                )

    def test_package_lock_rejects_wrong_registry_source(self) -> None:
        self._assert_package_lock_mutation_rejected(
            "resolved from",
            source_overrides={
                "jellyflow-core": "registry+https://github.com/rust-lang/crates.io-index"
            },
        )

    def test_package_lock_rejects_wrong_archive_checksum(self) -> None:
        self._assert_package_lock_mutation_rejected(
            "checksum does not match",
            checksum_overrides={"jellyflow-core": "0" * 64},
        )

    def test_package_lock_rejects_duplicate_candidate_identity(self) -> None:
        self._assert_package_lock_mutation_rejected(
            "contains 2 sources",
            duplicate_names=("jellyflow-core",),
        )

    def test_package_lock_rejects_non_candidate_version(self) -> None:
        self._assert_package_lock_mutation_rejected(
            "non-candidate Jellyflow packages",
            version_overrides={"jellyflow-core": "0.0.1-test"},
        )

    def test_source_path_scenario_uses_direct_test_dependencies(self) -> None:
        scenario = external_smoke.jellyflow_open_gpui_source_scenario(
            Path("/tmp/jellyflow-source")
        )
        dependencies = tomllib.loads(scenario.cargo_toml)["dependencies"]

        self.assertEqual(scenario.cargo_subcommand, "test")
        self.assertEqual(
            set(dependencies),
            {
                "jellyflow-core",
                "jellyflow-open-gpui",
                "jellyflow-runtime",
                "open_gpui",
                "open_gpui_ui_components",
            },
        )
        self.assertEqual(dependencies["open_gpui"]["features"], ["test-support"])
        self.assertFalse(dependencies["open_gpui_ui_components"]["default-features"])

    def test_adapter_conformance_consumer_is_direct_and_framework_neutral(self) -> None:
        scenario = external_smoke.jellyflow_adapter_conformance_source_scenario(
            Path("/tmp/jellyflow-source")
        )
        source_scenarios = external_smoke.jellyflow_source_path_smoke_scenarios(
            Path("/tmp/jellyflow-source")
        )
        manifest = tomllib.loads(scenario.cargo_toml)
        dependencies = manifest["dependencies"]

        self.assertEqual(scenario.cargo_subcommand, "test")
        self.assertIn(scenario.slug, {candidate.slug for candidate in source_scenarios})
        self.assertEqual(set(dependencies), {"jellyflow-core", "jellyflow-runtime"})
        self.assertEqual(
            dependencies["jellyflow-runtime"]["features"],
            ["adapter-conformance"],
        )

if __name__ == "__main__":
    unittest.main()
