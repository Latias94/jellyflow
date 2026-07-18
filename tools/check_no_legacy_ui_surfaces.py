#!/usr/bin/env python3
"""Reject superseded UI authoring surfaces from active Jellyflow code and docs."""

from __future__ import annotations

import re
import shlex
import sys
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class ForbiddenPattern:
    name: str
    expression: re.Pattern[str]


FORBIDDEN_PATHS = (
    ".agents/skills/trellis-before-dev",
    ".agents/skills/trellis-brainstorm",
    ".agents/skills/trellis-break-loop",
    ".agents/skills/trellis-check",
    ".agents/skills/trellis-continue",
    ".agents/skills/trellis-finish-work",
    ".agents/skills/trellis-meta",
    ".agents/skills/trellis-spec-bootstarp",
    ".agents/skills/trellis-start",
    ".agents/skills/trellis-update-spec",
    ".codex/agents/trellis-check.toml",
    ".codex/agents/trellis-implement.toml",
    ".codex/agents/trellis-research.toml",
    ".codex/config.toml",
    ".codex/hooks.json",
    ".codex/hooks/inject-workflow-state.py",
    ".codex/hooks/session-start.py",
    ".trellis",
    "crates/jellyflow-proof",
    "templates/headless-adapter",
    "docs/adr",
    "docs/examples",
    "crates/jellyflow-core/examples/build_graph.rs",
    "crates/jellyflow-layout/examples/dugong_timing.rs",
    "crates/jellyflow-egui/src/app.rs",
    "crates/jellyflow-egui/src/bridge.rs",
    "crates/jellyflow-egui/src/handle_layout.rs",
    "crates/jellyflow-egui/src/input.rs",
    "crates/jellyflow-egui/src/renderer.rs",
    "crates/jellyflow-egui/src/samples.rs",
    "crates/jellyflow-egui/src/state.rs",
    "crates/jellyflow-egui/src/ui",
    "crates/jellyflow-egui/examples/automation_builder.rs",
    "crates/jellyflow-egui/examples/custom_widget.rs",
    "crates/jellyflow-egui/examples/demo.rs",
    "crates/jellyflow-egui/examples/erd.rs",
    "crates/jellyflow-egui/examples/gallery_snapshot.rs",
    "crates/jellyflow-egui/examples/knowledge_board.rs",
    "crates/jellyflow-egui/examples/mind_map.rs",
    "crates/jellyflow-egui/examples/org_chart.rs",
    "crates/jellyflow-egui/examples/shader_graph.rs",
    "crates/jellyflow-egui/examples/tree.rs",
    "crates/jellyflow-egui/examples/workflow.rs",
    "crates/jellyflow-runtime/benches/layout_pipeline.rs",
    "crates/jellyflow-runtime/benches/rendering_query.rs",
    "crates/jellyflow-runtime/benches/schema_create_node.rs",
    "crates/jellyflow-runtime/examples/conformance_harness.rs",
    "crates/jellyflow-runtime/examples/dirty_scope_layout.rs",
    "crates/jellyflow-runtime/examples/dugong_layout.rs",
    "crates/jellyflow-runtime/examples/geometry_edge.rs",
    "crates/jellyflow-runtime/examples/knowledge_canvas.rs",
    "crates/jellyflow-runtime/examples/layout_engines.rs",
    "crates/jellyflow-runtime/examples/store_dispatch.rs",
    "crates/jellyflow-runtime/src/runtime/measurement.rs",
    "crates/jellyflow-runtime/src/runtime/binding",
    "crates/jellyflow-runtime/src/runtime/conformance",
    "crates/jellyflow-runtime/src/runtime/query",
    "crates/jellyflow-runtime/src/runtime/store",
    "crates/jellyflow-runtime/src/runtime/xyflow",
    "crates/jellyflow-runtime/src/profile/domain.rs",
    "crates/jellyflow-runtime/src/schema/types.rs",
    "crates/jellyflow-runtime/src/schema/kit",
    "crates/jellyflow-runtime/src/schema/registry",
    "crates/jellyflow-open-gpui/src/actions.rs",
    "crates/jellyflow-open-gpui/src/adapter.rs",
    "crates/jellyflow-open-gpui/src/authoring.rs",
    "crates/jellyflow-open-gpui/src/controls.rs",
    "crates/jellyflow-open-gpui/src/inspector.rs",
    "crates/jellyflow-open-gpui/src/json_binding.rs",
    "crates/jellyflow-open-gpui/src/measurement.rs",
    "crates/jellyflow-open-gpui/src/presets.rs",
    "crates/jellyflow-open-gpui/src/projection.rs",
    "crates/jellyflow-open-gpui/src/renderer.rs",
    "crates/jellyflow-open-gpui/src/repeatable.rs",
    "crates/jellyflow-open-gpui/src/sync.rs",
    "crates/jellyflow-open-gpui/src/testing.rs",
    "examples/canvas-jellyflow/src/gallery_screenshot.rs",
    "examples/canvas-jellyflow/src/measurement_bridge.rs",
    "examples/canvas-jellyflow/src/native_smoke.rs",
    "examples/canvas-jellyflow/src/node_component_kit.rs",
    "examples/canvas-jellyflow/src/product_gallery.rs",
    "examples/canvas-jellyflow/src/product_renderers.rs",
    "examples/canvas-jellyflow/src/visual_regression.rs",
)

SOURCE_GLOBS = (
    "crates/**/*.rs",
    "examples/canvas-jellyflow/src/**/*.rs",
    "examples/canvas-jellyflow/tests/**/*.rs",
    "templates/native-adapter/src/**/*.rs",
    "templates/native-adapter/tests/**/*.rs",
    "tools/fixtures/external-consumer/**/*.rs",
)

CONFIG_GLOBS = (
    "Cargo.toml",
    "crates/*/Cargo.toml",
    "examples/*/Cargo.toml",
    "templates/*/Cargo.toml",
)

HEADLESS_SOURCE_GLOBS = (
    "crates/jellyflow-core/**/*.rs",
    "crates/jellyflow-layout/**/*.rs",
    "crates/jellyflow-runtime/**/*.rs",
)

DOC_GLOBS = (
    "README.md",
    "CONTEXT.md",
    "crates/*/README.md",
    "examples/*/README.md",
    "templates/*/README.md",
    "docs/**/*.md",
)

HISTORICAL_DOC_ROOTS = (
    Path("docs/history"),
    Path("docs/plans"),
    Path("docs/research"),
)

SOURCE_PATTERNS = (
    ForbiddenPattern("NodeGraphStore", re.compile(r"\bNodeGraphStore\b")),
    ForbiddenPattern("legacy node schema", re.compile(r"\bNodeSchema\w*\b")),
    ForbiddenPattern("portable node UI type", re.compile(r"\b(?:NodeUi|PortableUi)\w*\b")),
    ForbiddenPattern("portable UI schema", re.compile(r"\bUiSchema\b")),
    ForbiddenPattern("portable node body", re.compile(r"\bNodeBody\w*\b")),
    ForbiddenPattern("JSON UI binding", re.compile(r"\bJsonBinding\w*\b")),
    ForbiddenPattern("old authoring facade", re.compile(r"\bOpenGpuiAuthoring\w*\b")),
    ForbiddenPattern(
        "portable layout budget",
        re.compile(
            r"\b(?:LayoutBudget|NodeSurfaceLayoutBudget|NodeUiLayoutBudget|"
            r"PortableUiLayoutBudget)\b"
        ),
    ),
    ForbiddenPattern(
        "old projection helper",
        re.compile(r"\b(?:NodeProjection|ProjectionHelper)\w*\b"),
    ),
    ForbiddenPattern("old repeatable planner", re.compile(r"\bProductRepeatable\w*\b")),
    ForbiddenPattern("old binding query", re.compile(r"\bBindingQuery\b")),
    ForbiddenPattern(
        "old interaction guard",
        re.compile(r"\b(?:OpenGpuiNodeInteractionGuard|OpenGpuiInteractionGuardElement)\b"),
    ),
    ForbiddenPattern(
        "adapter test backdoor",
        re.compile(
            r"\b(?:adapter_test_support|with_state_borrowed_for_adapter_test|"
            r"with_runtime_state_borrowed)\b|\badapter-test-support\b"
        ),
    ),
    ForbiddenPattern(
        "portable node descriptor DSL",
        re.compile(
            r"\b(?:NodeKindViewDescriptor|PortViewDescriptor|NodeKit\w*|NodeSurface\w*|"
            r"NodeControl\w*|NodeAction\w*|NodeRepeatable\w*|NodeChrome\w*|"
            r"MenuDescriptor|InspectorDescriptor|BlackboardDescriptor|PortHandleVisibility|"
            r"PortViewSide|PortDecl|ActionTarget|"
            r"ActionIntent|ActionAvailability|ActionShortcut|MenuSurface|InspectorTarget)\b"
        ),
    ),
    ForbiddenPattern(
        "portable Open GPUI projection",
        re.compile(
            r"\b(?:OpenGpuiNodeSurfaceLayout|OpenGpuiNodeSurfaceSlotLayout|"
            r"OpenGpuiRepeatableSurfaceLayout|OpenGpuiRepeatableSurfaceProjection|"
            r"OpenGpuiRepeatableItemLayout|OpenGpuiRepeatableItemProjection|"
            r"OpenGpuiProjectionFallbackStoreEvidence|OpenGpuiProjectionFallbackStoreSummary|"
            r"OpenGpuiProjectionMeasurementSource|OpenGpuiNodeSurfacePlan)\b"
        ),
    ),
    ForbiddenPattern(
        "portable Open GPUI projection helper",
        re.compile(
            r"\b(?:project_node_measurement|projected_node_surface_component_layout|"
            r"projected_node_surface_graph_layout|repeatable_surface_projection|"
            r"slot_projection_visibility|measured_surface_anchors|measured_surface_slots|"
            r"open_gpui_node_surface_plan|open_gpui_renderer_repeatable_items|"
            r"open_gpui_renderer_repeatable_surfaces)\b"
        ),
    ),
    ForbiddenPattern(
        "portable profile UI metadata",
        re.compile(
            r"\b(?:GraphProfileMetadata|NodeFieldSchemaSet|FieldSchema|ValidationHint|"
            r"VariableSurfaceDescriptor|VariableDescriptor|ConnectionRuleDescriptor)\b"
        ),
    ),
)

CONFIG_PATTERNS = (
    ForbiddenPattern(
        "adapter test-support feature",
        re.compile(r"\badapter[-_]test[-_]support\b"),
    ),
)

BINDING_SOURCE_PATTERNS = (
    ForbiddenPattern(
        "synchronous Open GPUI binding execute",
        re.compile(r"\b(?:pub(?:\([^)]*\))?\s+)?fn\s+execute\s*\("),
    ),
)

HEADLESS_SOURCE_PATTERNS = (
    ForbiddenPattern(
        "headless focusability",
        re.compile(r"\bfocusable\b|\bSet(?:Node|Edge)Focusable\b"),
    ),
)

DOC_PATTERNS = (
    ForbiddenPattern(
        "ADR registry link",
        re.compile(r"(?:\]\([^)]*docs/adr/|^\s*\[[^]]+\]:\s*\S*docs/adr/)"),
    ),
    ForbiddenPattern("deleted proof crate", re.compile(r"\bjellyflow-proof\b")),
    ForbiddenPattern("deleted adapter template", re.compile(r"templates/headless-adapter")),
    ForbiddenPattern(
        "portable UI implementation claim",
        re.compile(
            r"(?<!not )\b(?:provide|provides|ship|ships|define|defines|expose|exposes|"
            r"support|supports|use|uses|offer|offers)\s+(?:a\s+|the\s+)?"
            r"(?:shared\s+|cross-framework\s+)?portable\s+(?:node\s+)?"
            r"(?:UI|widget(?:\s+tree)?|renderer)\b",
            re.IGNORECASE,
        ),
    ),
)

SOURCE_LINE_ALLOWLIST = {
    "crates/jellyflow/tests/public_surface.rs": (
        re.compile(r'\s*assert!\(!prelude\.contains\("NodeGraphStore"\)\);\s*'),
    ),
    "crates/jellyflow-runtime/src/io/files/graph.rs": (
        re.compile(
            r'\s*"legacy NodeSchema descriptor `\{kind\}` at kind version '
            r'\{latest_kind_version\} is not a supported graph document: \{path\}"\s*'
        ),
    ),
}

HEADLESS_SOURCE_LINE_ALLOWLIST: Mapping[str, tuple[re.Pattern[str], ...]] = {
    "crates/jellyflow-runtime/src/io/tests/graph_file/support.rs": (
        re.compile(
            r'\s*pub\(super\) const REMOVED_FOCUSABILITY_KEY: &str = "focusable";\s*'
        ),
    ),
}


def collect_matching_files(repo_root: Path, patterns: tuple[str, ...]) -> list[Path]:
    return sorted(
        {
            candidate
            for pattern in patterns
            for candidate in repo_root.glob(pattern)
            if candidate.is_file()
        }
    )


def collect_source_files(repo_root: Path) -> list[Path]:
    return collect_matching_files(repo_root, SOURCE_GLOBS)


def collect_headless_source_files(repo_root: Path) -> list[Path]:
    return collect_matching_files(repo_root, HEADLESS_SOURCE_GLOBS)


def collect_config_files(repo_root: Path) -> list[Path]:
    return collect_matching_files(repo_root, CONFIG_GLOBS)


def collect_doc_files(repo_root: Path) -> list[Path]:
    candidates = collect_matching_files(repo_root, DOC_GLOBS)
    return [
        path
        for path in candidates
        if not any(
            historical_root == relative or historical_root in relative.parents
            for historical_root in HISTORICAL_DOC_ROOTS
            for relative in (path.relative_to(repo_root),)
        )
    ]


def forbidden_path_has_content(path: Path) -> bool:
    if path.is_file() or path.is_symlink():
        return True
    return path.is_dir() and any(candidate.is_file() or candidate.is_symlink() for candidate in path.rglob("*"))


def pattern_violations(
    repo_root: Path,
    paths: list[Path],
    patterns: tuple[ForbiddenPattern, ...],
    line_allowlist: Mapping[str, tuple[re.Pattern[str], ...]] | None = None,
) -> list[str]:
    violations: list[str] = []
    line_allowlist = line_allowlist or {}
    for path in paths:
        relative = path.relative_to(repo_root).as_posix()
        allowed_lines = line_allowlist.get(relative, ())
        text = path.read_text(encoding="utf-8")
        for line_number, line in enumerate(text.splitlines(), start=1):
            if any(expression.fullmatch(line) for expression in allowed_lines):
                continue
            for pattern in patterns:
                if pattern.expression.search(line):
                    violations.append(
                        f"{relative}:{line_number}: {pattern.name}: {line.strip()}"
                    )
    return violations


def markdown_fenced_commands(text: str) -> list[tuple[int, str, list[str]]]:
    commands: list[tuple[int, str, list[str]]] = []
    in_fence = False
    command_start = 0
    command_parts: list[str] = []

    for line_number, line in enumerate(text.splitlines(), start=1):
        stripped = line.strip()
        if stripped.startswith("```"):
            in_fence = not in_fence
            command_parts.clear()
            continue
        if not in_fence or not stripped or stripped.startswith("#"):
            continue

        if not command_parts:
            command_start = line_number
        continued = stripped.endswith("\\")
        command_parts.append(stripped[:-1].rstrip() if continued else stripped)
        if continued:
            continue

        command = " ".join(command_parts)
        command_parts.clear()
        if command.startswith("$ "):
            command = command[2:].lstrip()
        try:
            tokens = shlex.split(command)
        except ValueError:
            continue
        commands.append((command_start, command, tokens))

    return commands


def cargo_features(tokens: list[str]) -> set[str]:
    features: set[str] = set()
    for index, token in enumerate(tokens):
        if token == "--features" and index + 1 < len(tokens):
            value = tokens[index + 1]
        elif token.startswith("--features="):
            value = token.partition("=")[2]
        else:
            continue
        features.update(value.replace(",", " ").split())
    return features


def documented_smoke_command_violations(
    repo_root: Path,
    paths: list[Path],
) -> list[str]:
    violations: list[str] = []
    for path in paths:
        relative = path.relative_to(repo_root).as_posix()
        text = path.read_text(encoding="utf-8")
        for line_number, command, tokens in markdown_fenced_commands(text):
            if len(tokens) < 2 or tokens[:2] != ["cargo", "run"]:
                continue
            if "--smoke" not in tokens:
                continue
            separator = tokens.index("--") if "--" in tokens else len(tokens)
            if "real-window-smoke" in cargo_features(tokens[:separator]):
                continue
            violations.append(
                f"{relative}:{line_number}: real-window smoke feature: "
                "cargo run command must include `--features real-window-smoke` "
                f"before `--`: {command}"
            )
    return violations


def find_legacy_surface_violations(repo_root: Path) -> list[str]:
    violations = [
        f"forbidden legacy path still exists: {relative}"
        for relative in FORBIDDEN_PATHS
        if forbidden_path_has_content(repo_root / relative)
    ]
    violations.extend(
        pattern_violations(
            repo_root,
            collect_source_files(repo_root),
            SOURCE_PATTERNS,
            SOURCE_LINE_ALLOWLIST,
        )
    )
    violations.extend(
        pattern_violations(
            repo_root,
            collect_headless_source_files(repo_root),
            HEADLESS_SOURCE_PATTERNS,
            HEADLESS_SOURCE_LINE_ALLOWLIST,
        )
    )
    violations.extend(
        pattern_violations(
            repo_root,
            collect_config_files(repo_root),
            CONFIG_PATTERNS,
        )
    )
    binding_source = repo_root / "crates/jellyflow-open-gpui/src/binding.rs"
    if binding_source.is_file():
        violations.extend(
            pattern_violations(
                repo_root,
                [binding_source],
                BINDING_SOURCE_PATTERNS,
            )
        )
    violations.extend(
        pattern_violations(
            repo_root,
            collect_doc_files(repo_root),
            DOC_PATTERNS,
        )
    )
    violations.extend(
        documented_smoke_command_violations(repo_root, collect_doc_files(repo_root))
    )
    return violations


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    violations = find_legacy_surface_violations(repo_root)

    if violations:
        print("Legacy UI surface gate failed:", file=sys.stderr)
        for violation in violations:
            print(f"- {violation}", file=sys.stderr)
        return 1

    print("No superseded UI authoring surfaces remain in active code or docs.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
