from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class DocumentationContractTests(unittest.TestCase):
    def test_readme_commands_use_opaque_versions_proved_by_package_consumers(self) -> None:
        root_readme = (ROOT / "README.md").read_text(encoding="utf-8")
        open_gpui_readme = (
            ROOT / "crates" / "jellyflow-open-gpui" / "README.md"
        ).read_text(encoding="utf-8")
        headless_consumer = (
            ROOT
            / "tools"
            / "fixtures"
            / "external-consumer"
            / "package-headless"
            / "src"
            / "main.rs"
        ).read_text(encoding="utf-8")
        open_gpui_consumer = (
            ROOT
            / "tools"
            / "fixtures"
            / "external-consumer"
            / "open-gpui-adoption"
            / "src"
            / "main.rs"
        ).read_text(encoding="utf-8")

        self.assertIn("runtime.snapshot()?.version()", root_readme)
        self.assertNotIn("runtime.snapshot()?.revision(),", root_readme)
        self.assertIn("authoring.version()", open_gpui_readme)
        self.assertNotIn("authoring.revision()", open_gpui_readme)
        self.assertIn("GraphCommand::topology(version", headless_consumer)
        self.assertIn("GraphCommand::set_node_geometry(", headless_consumer)
        self.assertIn("authoring.version()", open_gpui_consumer)

    def test_real_window_docs_use_orchestrator_capture_output(self) -> None:
        paths = (
            ROOT / "docs" / "testing" / "node-ui-authoring-regression.md",
            ROOT / "docs" / "releasing" / "CRATES_IO.md",
        )

        for path in paths:
            source = path.read_text(encoding="utf-8")
            self.assertIn("python3 tools/run_real_window_smoke_matrix.py", source)
            self.assertIn("--receipt-validator", source)
            self.assertIn("smoke_receipt_contract", source)
            self.assertIn("--capture-dir-output", source)
            self.assertIn("--capture-dir-from-output", source)
        self.assertNotIn("run_smoke()", paths[0].read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
