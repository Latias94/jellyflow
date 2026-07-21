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
            "package-derived",
            "open-gpui-source",
        ):
            with self.subTest(fixture_name=fixture_name):
                expected = (FIXTURE_ROOT / fixture_name / "src/main.rs").read_text(
                    encoding="utf-8"
                )

                self.assertEqual(
                    external_smoke._load_external_consumer_main_rs(fixture_name),
                    expected,
                )

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
                    "assert module.jellyflow_package_consumer_scenario(versions).main_rs "
                    "== module._load_external_consumer_main_rs('package-derived')"
                ),
                (
                    "assert module.jellyflow_open_gpui_source_scenario("
                    "pathlib.Path('/tmp/source')).main_rs "
                    "== module._load_external_consumer_main_rs('open-gpui-source')"
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

    def test_package_scenario_constructs_the_published_open_gpui_host(self) -> None:
        versions = {name: "0.0.0-test" for name in external_smoke.PUBLISHABLE_CRATES}

        scenario = external_smoke.jellyflow_package_consumer_scenario(versions)
        manifest = tomllib.loads(scenario.cargo_toml)
        dependencies = manifest["dependencies"]

        self.assertEqual(len(external_smoke.PUBLISHABLE_CRATES), 5)
        self.assertEqual(
            set(external_smoke.PUBLISHABLE_CRATES),
            set(external_smoke.PUBLISHABLE_CRATES).intersection(dependencies),
        )
        for name in external_smoke.PUBLISHABLE_CRATES:
            self.assertEqual(
                dependencies[name],
                {
                    "version": "=0.0.0-test",
                    "registry": "jellyflow-package-archives",
                },
            )
        self.assertEqual(
            dependencies["open_gpui"],
            {
                "package": "open-gpui",
                "version": "=0.2.0",
                "default-features": False,
            },
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
            },
        )
        self.assertEqual(dependencies["open_gpui"]["features"], ["test-support"])

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
        self.assertNotIn("jellyflow", dependencies)
        self.assertNotIn("jellyflow-egui", dependencies)
        self.assertNotIn("jellyflow-open-gpui", dependencies)

if __name__ == "__main__":
    unittest.main()
