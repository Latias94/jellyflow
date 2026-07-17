from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "check_no_legacy_ui_surfaces.py"
REPO_ROOT = SCRIPT_PATH.parents[1]
SPEC = importlib.util.spec_from_file_location("check_no_legacy_ui_surfaces", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {SCRIPT_PATH}")
legacy_gate = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = legacy_gate
SPEC.loader.exec_module(legacy_gate)


def source_violations(source: str, relative: str = "crates/example/src/lib.rs") -> list[str]:
    with tempfile.TemporaryDirectory() as directory:
        repo_root = Path(directory)
        path = repo_root / relative
        path.parent.mkdir(parents=True)
        path.write_text(source, encoding="utf-8")
        return legacy_gate.pattern_violations(
            repo_root,
            [path],
            legacy_gate.SOURCE_PATTERNS,
            legacy_gate.SOURCE_LINE_ALLOWLIST,
        )


def headless_source_violations(
    source: str,
    relative: str = "crates/jellyflow-runtime/src/lib.rs",
) -> list[str]:
    with tempfile.TemporaryDirectory() as directory:
        repo_root = Path(directory)
        path = repo_root / relative
        path.parent.mkdir(parents=True)
        path.write_text(source, encoding="utf-8")
        return legacy_gate.pattern_violations(
            repo_root,
            [path],
            legacy_gate.HEADLESS_SOURCE_PATTERNS,
            legacy_gate.HEADLESS_SOURCE_LINE_ALLOWLIST,
        )


def config_violations(
    source: str,
    relative: str = "crates/jellyflow-runtime/Cargo.toml",
) -> list[str]:
    with tempfile.TemporaryDirectory() as directory:
        repo_root = Path(directory)
        path = repo_root / relative
        path.parent.mkdir(parents=True)
        path.write_text(source, encoding="utf-8")
        return legacy_gate.pattern_violations(
            repo_root,
            [path],
            legacy_gate.CONFIG_PATTERNS,
        )


def binding_source_violations(source: str) -> list[str]:
    with tempfile.TemporaryDirectory() as directory:
        repo_root = Path(directory)
        path = repo_root / "crates/jellyflow-open-gpui/src/binding.rs"
        path.parent.mkdir(parents=True)
        path.write_text(source, encoding="utf-8")
        return legacy_gate.pattern_violations(
            repo_root,
            [path],
            legacy_gate.BINDING_SOURCE_PATTERNS,
        )


def write_repo_file(repo_root: Path, relative: str, contents: str) -> Path:
    path = repo_root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents, encoding="utf-8")
    return path


class LegacyUiSurfaceTests(unittest.TestCase):
    def test_headless_focusability_symbols_are_forbidden(self) -> None:
        for symbol in ("focusable", "SetNodeFocusable", "SetEdgeFocusable"):
            with self.subTest(symbol=symbol):
                violations = headless_source_violations(
                    f"fn regression() {{ let _ = {symbol}; }}\n"
                )

                self.assertEqual(len(violations), 1)
                self.assertIn("headless focusability", violations[0])

    def test_native_adapter_focusability_is_outside_the_headless_gate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            repo_root = Path(directory)
            write_repo_file(
                repo_root,
                "crates/jellyflow-open-gpui/src/input.rs",
                "fn native_input() { div().focusable(); }\n",
            )

            files = legacy_gate.collect_headless_source_files(repo_root)

        self.assertEqual(files, [])

    def test_graph_file_regression_allowlist_is_only_line_scoped(self) -> None:
        violations = headless_source_violations(
            '\n'.join(
                (
                    'pub(super) const REMOVED_FOCUSABILITY_KEY: &str = "focusable";',
                    'let focusable = true;',
                )
            ),
            "crates/jellyflow-runtime/src/io/tests/graph_file/support.rs",
        )

        self.assertEqual(len(violations), 1)
        self.assertIn("support.rs:2: headless focusability", violations[0])

    def test_discovery_includes_future_workspace_crate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            repo_root = Path(directory)
            write_repo_file(
                repo_root,
                "crates/future-native-host/src/nested/lib.rs",
                "use jellyflow_runtime::NodeGraphStore;\n",
            )

            violations = legacy_gate.find_legacy_surface_violations(repo_root)

        self.assertTrue(
            any(
                "crates/future-native-host/src/nested/lib.rs" in violation
                and "NodeGraphStore" in violation
                for violation in violations
            ),
            violations,
        )

    def test_deleted_trellis_codex_config_files_are_forbidden(self) -> None:
        for relative in (".codex/config.toml", ".codex/hooks.json"):
            with self.subTest(relative=relative), tempfile.TemporaryDirectory() as directory:
                repo_root = Path(directory)
                write_repo_file(repo_root, relative, "{}\n")

                violations = legacy_gate.find_legacy_surface_violations(repo_root)

                self.assertIn(
                    f"forbidden legacy path still exists: {relative}",
                    violations,
                )

    def test_discovery_rejects_legacy_dsl_in_external_consumer_fixture(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            repo_root = Path(directory)
            write_repo_file(
                repo_root,
                "tools/fixtures/external-consumer/legacy/src/main.rs",
                "use jellyflow::NodeKindViewDescriptor;\n",
            )

            violations = legacy_gate.find_legacy_surface_violations(repo_root)

        self.assertTrue(
            any(
                "tools/fixtures/external-consumer/legacy/src/main.rs" in violation
                and "portable node descriptor DSL" in violation
                for violation in violations
            ),
            violations,
        )

    def test_public_external_consumer_fixtures_are_scanned_and_clean(self) -> None:
        fixture_root = REPO_ROOT / "tools/fixtures/external-consumer"
        expected = set(fixture_root.glob("**/*.rs"))
        discovered = set(legacy_gate.collect_source_files(REPO_ROOT))

        self.assertTrue(expected)
        self.assertTrue(expected.issubset(discovered), discovered)
        self.assertEqual(
            legacy_gate.pattern_violations(
                REPO_ROOT,
                sorted(expected),
                legacy_gate.SOURCE_PATTERNS,
                legacy_gate.SOURCE_LINE_ALLOWLIST,
            ),
            [],
        )

    def test_discovery_includes_nested_active_document(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            repo_root = Path(directory)
            write_repo_file(
                repo_root,
                "docs/testing/guides/new-adapter.md",
                "Jellyflow provides a portable UI renderer.\n",
            )

            violations = legacy_gate.find_legacy_surface_violations(repo_root)

        self.assertTrue(
            any(
                "docs/testing/guides/new-adapter.md" in violation
                and "portable UI implementation claim" in violation
                for violation in violations
            ),
            violations,
        )

    def test_discovery_excludes_historical_plan_and_research_corpus(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            repo_root = Path(directory)
            for relative in (
                "docs/history/retired-adapter.md",
                "docs/plans/retired-portable-ui.md",
                "docs/research/portable-ui-options.md",
            ):
                write_repo_file(
                    repo_root,
                    relative,
                    "Jellyflow provides a portable UI renderer.\n",
                )

            violations = legacy_gate.find_legacy_surface_violations(repo_root)

        self.assertEqual(violations, [])

    def test_adapter_test_backdoors_are_forbidden_in_source_and_manifests(self) -> None:
        for symbol in (
            "adapter_test_support",
            "with_state_borrowed_for_adapter_test",
            "with_runtime_state_borrowed",
        ):
            with self.subTest(symbol=symbol):
                violations = source_violations(f"fn regression() {{ {symbol}(); }}\n")
                self.assertEqual(len(violations), 1)
                self.assertIn("adapter test backdoor", violations[0])

        for feature in ("adapter-test-support", "adapter_test_support"):
            with self.subTest(feature=feature):
                violations = config_violations(
                    f'[features]\n{feature!r} = []\n'
                )
                self.assertEqual(len(violations), 1)
                self.assertIn("adapter test-support feature", violations[0])

    def test_synchronous_open_gpui_binding_execute_is_forbidden(self) -> None:
        for signature in (
            "pub fn execute(&self) {}",
            "pub(crate) fn execute(&self) {}",
            "fn execute(&self) {}",
        ):
            with self.subTest(signature=signature):
                violations = binding_source_violations(signature)
                self.assertEqual(len(violations), 1)
                self.assertIn("synchronous Open GPUI binding execute", violations[0])

        self.assertEqual(source_violations("pub fn execute(&self) {}\n"), [])

    def test_documented_smoke_command_requires_feature_before_separator(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            repo_root = Path(directory)
            write_repo_file(
                repo_root,
                "docs/testing/native-window.md",
                "# Native window\n\n"
                "```sh\n"
                "cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml \\\n"
                "  -p open-gpui-canvas-jellyflow --locked -- --smoke\n"
                "```\n",
            )

            violations = legacy_gate.find_legacy_surface_violations(repo_root)

        self.assertTrue(
            any("real-window smoke feature" in violation for violation in violations),
            violations,
        )

    def test_public_documented_smoke_commands_enable_required_feature(self) -> None:
        violations = legacy_gate.documented_smoke_command_violations(
            REPO_ROOT,
            legacy_gate.collect_doc_files(REPO_ROOT),
        )

        self.assertEqual(violations, [])

    def test_plan_named_portable_dsl_and_projection_symbols_are_forbidden(self) -> None:
        cases = {
            "NodeSchema": "legacy node schema",
            "NodeSchemaBuilder": "legacy node schema",
            "LayoutBudget": "portable layout budget",
            "NodeSurfaceLayoutBudget": "portable layout budget",
            "NodeKindViewDescriptor": "portable node descriptor DSL",
            "PortViewDescriptor": "portable node descriptor DSL",
            "NodeControlDescriptor": "portable node descriptor DSL",
            "NodeRepeatableCollectionDescriptor": "portable node descriptor DSL",
            "NodeActionDescriptor": "portable node descriptor DSL",
            "MenuDescriptor": "portable node descriptor DSL",
            "InspectorDescriptor": "portable node descriptor DSL",
            "BlackboardDescriptor": "portable node descriptor DSL",
            "PortHandleVisibility": "portable node descriptor DSL",
            "PortViewSide": "portable node descriptor DSL",
            "PortDecl": "portable node descriptor DSL",
            "NodeChromeDescriptor": "portable node descriptor DSL",
            "NodeSurfaceProjection": "portable node descriptor DSL",
            "NodeSurfaceSlotProjection": "portable node descriptor DSL",
            "OpenGpuiNodeSurfaceLayout": "portable Open GPUI projection",
            "OpenGpuiRepeatableSurfaceProjection": "portable Open GPUI projection",
            "OpenGpuiProjectionFallbackStoreEvidence": "portable Open GPUI projection",
            "OpenGpuiNodeSurfacePlan": "portable Open GPUI projection",
            "project_node_measurement": "portable Open GPUI projection helper",
            "projected_node_surface_component_layout": (
                "portable Open GPUI projection helper"
            ),
            "repeatable_surface_projection": "portable Open GPUI projection helper",
            "slot_projection_visibility": "portable Open GPUI projection helper",
        }

        for symbol, expected_name in cases.items():
            with self.subTest(symbol=symbol):
                violations = source_violations(f"fn regression() {{ {symbol}; }}\n")
                self.assertTrue(
                    any(f": {expected_name}:" in violation for violation in violations),
                    violations,
                )

    def test_public_surface_negative_assertion_is_only_line_scoped(self) -> None:
        source = "\n".join(
            (
                'assert!(!prelude.contains("NodeGraphStore"));',
                "use jellyflow::NodeGraphStore;",
            )
        )

        violations = source_violations(
            source,
            "crates/jellyflow/tests/public_surface.rs",
        )

        self.assertEqual(len(violations), 1)
        self.assertIn("public_surface.rs:2: NodeGraphStore", violations[0])

    def test_allowlisted_assertion_cannot_hide_an_extra_legacy_symbol(self) -> None:
        violations = source_violations(
            'assert!(!prelude.contains("NodeGraphStore")); let _: NodeSchema;\n',
            "crates/jellyflow/tests/public_surface.rs",
        )

        self.assertEqual(len(violations), 2)

    def test_legacy_file_error_text_does_not_allow_the_removed_type(self) -> None:
        source = "\n".join(
            (
                '"legacy NodeSchema descriptor `{kind}` at kind version '
                '{latest_kind_version} is not a supported graph document: {path}"',
                "use jellyflow_runtime::NodeSchema;",
            )
        )

        violations = source_violations(
            source,
            "crates/jellyflow-runtime/src/io/files/graph.rs",
        )

        self.assertEqual(len(violations), 1)
        self.assertIn("graph.rs:2: legacy node schema", violations[0])

    def test_renderer_neutral_layout_names_are_not_rejected(self) -> None:
        source = "\n".join(
            (
                "struct MindMapProjection;",
                "struct EdgeViewDescriptor;",
                "struct PortPresentationHint;",
            )
        )

        self.assertEqual(source_violations(source), [])


if __name__ == "__main__":
    unittest.main()
