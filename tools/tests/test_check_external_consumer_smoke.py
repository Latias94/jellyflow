from __future__ import annotations

import importlib.util
import io
import sys
import tarfile
import tempfile
import tomllib
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "check_external_consumer_smoke.py"
SPEC = importlib.util.spec_from_file_location("check_external_consumer_smoke", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {SCRIPT_PATH}")
external_smoke = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = external_smoke
SPEC.loader.exec_module(external_smoke)


class ExternalConsumerScenarioTests(unittest.TestCase):
    @staticmethod
    def _cargo_metadata(
        *package_names: str,
        features: dict[str, list[str]] | None = None,
    ) -> dict[str, object]:
        packages = [
            {"id": f"{name} 0.0.0", "name": name} for name in package_names
        ]
        features = features or {}
        return {
            "packages": packages,
            "resolve": {
                "nodes": [
                    {
                        "id": package["id"],
                        "features": features.get(package["name"], []),
                    }
                    for package in packages
                ]
            },
        }

    def test_packaged_licenses_must_exist_and_match_workspace_text(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "package"
            source.mkdir()
            for name in external_smoke.LICENSE_FILES:
                (root / name).write_text(f"canonical {name}\n", encoding="utf-8")
                (source / name).write_text(f"canonical {name}\n", encoding="utf-8")

            external_smoke.validate_packaged_licenses(root, source)

            missing = external_smoke.LICENSE_FILES[0]
            (source / missing).unlink()
            with self.assertRaisesRegex(RuntimeError, "is missing"):
                external_smoke.validate_packaged_licenses(root, source)

            (source / missing).write_text("wrong\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "does not match"):
                external_smoke.validate_packaged_licenses(root, source)

    def test_archive_patch_config_uses_extracted_sources_and_shared_target(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            packaged = [
                external_smoke.PackagedCrate(
                    name=name,
                    version="0.0.0-test",
                    source_dir=root / "extracted" / name,
                )
                for name in ("jellyflow-core", "jellyflow-runtime")
            ]
            target_dir = root / "target"

            config = tomllib.loads(
                external_smoke.package_archive_patch_config(packaged, target_dir)
            )

            self.assertEqual(
                config["patch"]["crates-io"],
                {
                    package.name: {"path": str(package.source_dir.resolve())}
                    for package in packaged
                },
            )
            self.assertEqual(config["build"]["target-dir"], str(target_dir.resolve()))

    def test_normalized_manifest_rejects_a_workspace_table(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text(
                '\n'.join(
                    [
                        '[package]',
                        'name = "fixture"',
                        'version = "0.0.0"',
                        '',
                        '[workspace]',
                        '',
                    ]
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(RuntimeError, r"retained \[workspace\]"):
                external_smoke.normalized_package_manifest(root)

    def test_package_extraction_rejects_links_and_special_members(self) -> None:
        for member_type in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.FIFOTYPE):
            with (
                self.subTest(member_type=member_type),
                tempfile.TemporaryDirectory() as directory,
            ):
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

    def test_package_scenarios_cover_three_public_consumers_and_release_set(self) -> None:
        versions = {name: "0.0.0-test" for name in external_smoke.PUBLISHABLE_CRATES}
        scenarios = external_smoke.jellyflow_package_consumer_scenarios(versions)

        self.assertEqual(
            [scenario.smoke.slug for scenario in scenarios],
            [
                "jellyflow-package-headless-smoke",
                "jellyflow-package-egui-smoke",
                "jellyflow-package-open-gpui-smoke",
            ],
        )
        self.assertEqual(
            {
                package
                for scenario in scenarios
                for package in scenario.expected_jellyflow_closure
            },
            set(external_smoke.PUBLISHABLE_CRATES),
        )
        self.assertEqual(
            [
                scenario.smoke.slug
                for scenario in scenarios
                if scenario.checks_open_gpui_features
            ],
            ["jellyflow-package-open-gpui-smoke"],
        )

    def test_package_metadata_requires_exact_closure_without_test_support(self) -> None:
        versions = {name: "0.0.0-test" for name in external_smoke.PUBLISHABLE_CRATES}
        scenario = next(
            scenario
            for scenario in external_smoke.jellyflow_package_consumer_scenarios(
                versions
            )
            if scenario.checks_open_gpui_features
        )
        package_names = (
            scenario.smoke.slug,
            *sorted(scenario.expected_jellyflow_closure),
            "open-gpui",
        )

        self.assertEqual(
            external_smoke.assert_package_metadata_boundary(
                self._cargo_metadata(*package_names), scenario
            ),
            set(scenario.expected_jellyflow_closure),
        )

        with self.assertRaisesRegex(RuntimeError, "expected"):
            external_smoke.assert_package_metadata_boundary(
                self._cargo_metadata(
                    scenario.smoke.slug,
                    "jellyflow-core",
                    "jellyflow-open-gpui",
                    "open-gpui",
                ),
                scenario,
            )

        for package in ("jellyflow-open-gpui", "open-gpui"):
            with self.subTest(package=package), self.assertRaisesRegex(
                RuntimeError, "test-support"
            ):
                external_smoke.assert_package_metadata_boundary(
                    self._cargo_metadata(
                        *package_names,
                        features={package: ["test-support"]},
                    ),
                    scenario,
                )

        with self.assertRaisesRegex(RuntimeError, "outside the release set"):
            external_smoke.assert_package_metadata_boundary(
                self._cargo_metadata(*package_names, "jellyflow-unknown"),
                scenario,
            )

    def test_archive_metadata_sources_require_extracted_candidate_sources(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            packaged = [
                external_smoke.PackagedCrate(
                    name=name,
                    version="0.0.0-test",
                    source_dir=root / "extracted" / name,
                )
                for name in ("jellyflow-core", "jellyflow-runtime")
            ]
            expected = frozenset(package.name for package in packaged)

            def metadata_for(
                candidates: list[tuple[str, str, Path]],
            ) -> dict[str, object]:
                return {
                    "packages": [
                        {
                            "name": name,
                            "version": version,
                            "manifest_path": str(source_dir / "Cargo.toml"),
                        }
                        for name, version, source_dir in candidates
                    ]
                }

            valid_candidates = [
                (package.name, package.version, package.source_dir)
                for package in packaged
            ]
            external_smoke.assert_archive_metadata_sources(
                metadata_for(valid_candidates),
                packaged,
                expected,
                "fixture-consumer",
            )

            wrong_source = list(valid_candidates)
            wrong_source[0] = (
                packaged[0].name,
                packaged[0].version,
                root / "workspace-source",
            )
            with self.assertRaisesRegex(RuntimeError, "resolved from"):
                external_smoke.assert_archive_metadata_sources(
                    metadata_for(wrong_source),
                    packaged,
                    expected,
                    "fixture-consumer",
                )

            duplicate = valid_candidates + [valid_candidates[0]]
            with self.assertRaisesRegex(RuntimeError, "2 candidates"):
                external_smoke.assert_archive_metadata_sources(
                    metadata_for(duplicate),
                    packaged,
                    expected,
                    "fixture-consumer",
                )

            alternate_version = valid_candidates + [
                (
                    packaged[0].name,
                    "0.0.0-older",
                    root / "other-version-source",
                )
            ]
            with self.assertRaisesRegex(RuntimeError, "2 candidates"):
                external_smoke.assert_archive_metadata_sources(
                    metadata_for(alternate_version),
                    packaged,
                    expected,
                    "fixture-consumer",
                )


if __name__ == "__main__":
    unittest.main()
