#!/usr/bin/env python3
"""Run a temporary external Jellyflow consumer outside the Fret workspace."""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
import textwrap
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class SmokeScenario:
    slug: str
    cargo_toml: str
    main_rs: str


def write_file(path: Path, contents: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents, encoding="utf-8")


def jellyflow_external_consumer_scenarios(repo_root: Path) -> list[SmokeScenario]:
    return [jellyflow_runtime_scenario(repo_root)]


def jellyflow_runtime_scenario(repo_root: Path) -> SmokeScenario:
    return SmokeScenario(
        slug="jellyflow-external-smoke",
        cargo_toml=jellyflow_runtime_cargo_toml(repo_root),
        main_rs=jellyflow_runtime_main_rs(),
    )


def jellyflow_runtime_cargo_toml(repo_root: Path) -> str:
    return (
        textwrap.dedent(
            f"""
            [package]
            name = "jellyflow-external-smoke"
            version = "0.0.0"
            edition = "2024"
            rust-version = "1.92"
            publish = false

            [dependencies]
            jellyflow-core = {{ path = "{(repo_root / "crates/jellyflow-core").as_posix()}" }}
            jellyflow-layout = {{ path = "{(repo_root / "crates/jellyflow-layout").as_posix()}" }}
            jellyflow-runtime = {{ path = "{(repo_root / "crates/jellyflow-runtime").as_posix()}" }}
            """
        ).strip()
        + "\n"
    )


def jellyflow_runtime_main_rs() -> str:
    return (
        textwrap.dedent(
            """
            use jellyflow_core::{
                CanvasPoint, CanvasRect, CanvasSize, Graph, GraphId, GraphOp, GraphTransaction,
                Node, NodeGraphModifierKey, NodeGraphModifiers, NodeId, NodeKindKey,
            };
            use jellyflow_layout::{
                LayoutContext, LayoutEngineId, LayoutEngineRequest, LayoutRequest,
                builtin_layout_engine_registry, layout_graph_with_engine,
            };
            use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
            use jellyflow_runtime::runtime::conformance::{
                run_conformance_scenario, ConformanceAction, ConformanceFixtureDirectory,
                ConformanceScenario, ConformanceSuite, ConformanceTraceEvent,
            };
            use jellyflow_runtime::runtime::fit_view::{
                compute_fit_view_target_for_canvas_rect, FitViewComputeOptions,
            };
            use jellyflow_runtime::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
            use jellyflow_runtime::runtime::geometry::{
                bezier_edge_path, edge_path_contains_point, edge_position, BezierEdgeOptions,
                EdgeEndpointInput, EdgeHitTestOptions, HandleBounds, HandlePosition,
            };
            use jellyflow_runtime::NodeGraphStore;

            fn make_node(kind: &str, x: f32, y: f32) -> Node {
                Node {
                    kind: NodeKindKey::new(kind),
                    kind_version: 1,
                    pos: CanvasPoint { x, y },
                    origin: None,
                    selectable: None,
                    focusable: None,
                    draggable: None,
                    connectable: None,
                    deletable: None,
                    parent: None,
                    extent: None,
                    expand_parent: None,
                    size: Some(CanvasSize {
                        width: 160.0,
                        height: 80.0,
                    }),
                    hidden: false,
                    collapsed: false,
                    ports: Vec::new(),
                    data: Default::default(),
                }
            }

            fn main() {
                let node_id = NodeId::from_u128(2);
                let node = make_node("demo.source", 10.0, 20.0);
                let mut graph = Graph::new(GraphId::from_u128(1));

                let mut add = GraphTransaction::new().with_label("add demo node");
                add.push(GraphOp::AddNode { id: node_id, node });
                add.apply_to(&mut graph).expect("transaction applies");
                assert!(graph.nodes.contains_key(&node_id));

                let mut store = NodeGraphStore::new(
                    graph,
                    NodeGraphViewState::default(),
                    NodeGraphEditorConfig::default(),
                );
                let mut move_node = GraphTransaction::new().with_label("move demo node");
                move_node.push(GraphOp::SetNodePos {
                    id: node_id,
                    from: CanvasPoint { x: 10.0, y: 20.0 },
                    to: CanvasPoint { x: 32.0, y: 48.0 },
                });
                let outcome = store
                    .dispatch_transaction(&move_node)
                    .expect("store dispatch succeeds");
                assert_eq!(outcome.committed().ops.len(), 1);
                assert_eq!(store.graph().nodes[&node_id].pos, CanvasPoint { x: 32.0, y: 48.0 });

                let layout_registry = builtin_layout_engine_registry();
                let layout_request = LayoutEngineRequest::dugong(LayoutRequest::all());
                let graph_layout = layout_graph_with_engine(
                    store.graph(),
                    &layout_request,
                    &layout_registry,
                    &LayoutContext::new(),
                )
                .expect("direct layout crate planning succeeds");
                assert!(graph_layout.node_position(node_id).is_some());

                let freeform_request = LayoutEngineRequest::new(
                    LayoutEngineId::mind_map_freeform(),
                    LayoutRequest::all(),
                );
                let freeform_layout = layout_graph_with_engine(
                    store.graph(),
                    &freeform_request,
                    &layout_registry,
                    &LayoutContext::new(),
                )
                .expect("freeform layout planning succeeds");
                assert!(freeform_layout.node_position(node_id).is_some());

                let runtime_layout = store
                    .plan_layout(&layout_request, &layout_registry)
                    .expect("runtime layout planning succeeds");
                assert!(runtime_layout.node_position(node_id).is_some());

                let conformance_scenario =
                    ConformanceScenario::new("external node drag fixture", store.graph().clone())
                        .with_actions([ConformanceAction::apply_node_drag(
                            node_id,
                            CanvasPoint { x: 96.0, y: 128.0 },
                        )])
                        .with_expected_trace([ConformanceTraceEvent::graph_commit(
                            Some(NODE_DRAG_TRANSACTION_LABEL),
                            ["set_node_pos"],
                        )]);
                let conformance_report = run_conformance_scenario(&conformance_scenario)
                    .expect("conformance fixture runs");
                assert!(conformance_report.is_match(), "{conformance_report}");

                let fixture_root = std::env::temp_dir().join(format!(
                    "jellyflow-external-fixtures-{}",
                    std::process::id()
                ));
                let suite_path = fixture_root.join("nested").join("suite.json");
                let suite = ConformanceSuite::new("external adapter suite")
                    .with_scenarios([conformance_scenario]);
                suite.save_json(&suite_path).expect("save conformance suite");
                let loaded_suite =
                    ConformanceSuite::load_json(&suite_path).expect("load conformance suite");
                let suite_report = loaded_suite.run();
                assert!(suite_report.is_match(), "{suite_report}");
                assert!(
                    ConformanceSuite::load_json_if_exists(&suite_path)
                        .expect("load optional conformance suite")
                        .is_some()
                );

                let fixture_directory = ConformanceFixtureDirectory::load_json(&fixture_root)
                    .expect("load conformance fixture directory");
                assert_eq!(fixture_directory.file_count(), 1);
                let directory_report = fixture_directory.run();
                assert!(directory_report.is_match(), "{directory_report}");
                assert_eq!(directory_report.scenario_count(), 1);
                assert!(
                    ConformanceFixtureDirectory::load_json_if_exists(&fixture_root)
                        .expect("load optional conformance fixture directory")
                        .is_some()
                );
                std::fs::remove_dir_all(&fixture_root).expect("remove conformance fixture directory");
                assert!(
                    ConformanceFixtureDirectory::load_json_if_exists(&fixture_root)
                        .expect("missing optional conformance fixture directory")
                        .is_none()
                );

                let (pan, zoom) = compute_fit_view_target_for_canvas_rect(
                    CanvasRect {
                        origin: CanvasPoint { x: 32.0, y: 48.0 },
                        size: CanvasSize {
                            width: 160.0,
                            height: 80.0,
                        },
                    },
                    FitViewComputeOptions {
                        viewport_width_px: 800.0,
                        viewport_height_px: 600.0,
                        node_origin: (0.0, 0.0),
                        padding: 0.1,
                        margin_px_fallback: 24.0,
                        min_zoom: 0.1,
                        max_zoom: 4.0,
                    },
                )
                .expect("fit-view target");
                assert!(pan.x.is_finite() && pan.y.is_finite());
                assert!(zoom.is_finite() && zoom > 0.0);

                let endpoints = edge_position(
                    EdgeEndpointInput {
                        node_rect: CanvasRect {
                            origin: CanvasPoint { x: 32.0, y: 48.0 },
                            size: CanvasSize {
                                width: 160.0,
                                height: 80.0,
                            },
                        },
                        handle: Some(HandleBounds {
                            rect: CanvasRect {
                                origin: CanvasPoint { x: 152.0, y: 32.0 },
                                size: CanvasSize {
                                    width: 8.0,
                                    height: 16.0,
                                },
                            },
                            position: HandlePosition::Right,
                        }),
                        fallback_position: HandlePosition::Right,
                    },
                    EdgeEndpointInput {
                        node_rect: CanvasRect {
                            origin: CanvasPoint { x: 320.0, y: 96.0 },
                            size: CanvasSize {
                                width: 160.0,
                                height: 80.0,
                            },
                        },
                        handle: Some(HandleBounds {
                            rect: CanvasRect {
                                origin: CanvasPoint { x: 0.0, y: 32.0 },
                                size: CanvasSize {
                                    width: 8.0,
                                    height: 16.0,
                                },
                            },
                            position: HandlePosition::Left,
                        }),
                        fallback_position: HandlePosition::Left,
                    },
                )
                .expect("edge endpoints");
                let edge_path = bezier_edge_path(
                    endpoints.source,
                    endpoints.target,
                    BezierEdgeOptions::default(),
                )
                .expect("edge path");
                assert!(edge_path_contains_point(
                    &edge_path,
                    edge_path.label.point,
                    EdgeHitTestOptions::default(),
                ));

                assert!(NodeGraphModifierKey::CtrlOrMeta.is_pressed(NodeGraphModifiers {
                    ctrl: true,
                    ..NodeGraphModifiers::default()
                }));
            }
            """
        ).strip()
        + "\n"
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


def run_cargo_smoke(repo_root: Path, project_dir: Path) -> None:
    run_cargo_manifest(repo_root, project_dir / "Cargo.toml")


def run_cargo_manifest(
    repo_root: Path,
    manifest_path: Path,
    extra_args: list[str] | None = None,
    capture_output: bool = False,
) -> subprocess.CompletedProcess[str]:
    command = [
        "cargo",
        "run",
        "--quiet",
        "--manifest-path",
        str(manifest_path),
    ]
    if extra_args:
        command.extend(["--", *extra_args])
    return subprocess.run(
        command,
        cwd=repo_root,
        check=True,
        capture_output=capture_output,
        text=capture_output,
    )


def cargo_tree(repo_root: Path, project_dir: Path) -> str:
    return cargo_tree_for_manifest(repo_root, project_dir / "Cargo.toml")


def cargo_tree_for_manifest(repo_root: Path, manifest_path: Path) -> str:
    tree = subprocess.run(
        [
            "cargo",
            "tree",
            "--manifest-path",
            str(manifest_path),
            "--prefix",
            "none",
        ],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )
    return tree.stdout


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


def run_scenario(repo_root: Path, project_dir: Path, scenario: SmokeScenario) -> bool:
    write_scenario(project_dir, scenario)
    print(f"external smoke project: {project_dir}", flush=True)
    run_cargo_smoke(repo_root, project_dir)
    return assert_no_fret_packages(cargo_tree(repo_root, project_dir))


def run_template_adapter_smoke(repo_root: Path) -> bool:
    template_manifest = repo_root / "templates/headless-adapter/Cargo.toml"
    print(f"headless adapter template smoke: {template_manifest.parent}", flush=True)
    result = run_cargo_manifest(
        repo_root,
        template_manifest,
        ["check"],
        capture_output=True,
    )
    if '"scenario_reports"' not in result.stdout:
        print(
            "headless adapter template did not print a suite report",
            file=sys.stderr,
        )
        return False
    print("headless adapter template check produced a suite report")
    return assert_no_fret_packages(cargo_tree_for_manifest(repo_root, template_manifest))


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Create a temporary Cargo project outside this workspace and run a smoke binary that can "
            "consume jellyflow-core and jellyflow-runtime without fret-node or fret-core."
        )
    )
    parser.add_argument(
        "--keep",
        action="store_true",
        help="keep the temporary smoke project for inspection",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[1]
    scenarios = jellyflow_external_consumer_scenarios(repo_root)
    temp_root, temp_context = make_temp_root(args.keep)

    try:
        ok = True
        for scenario in scenarios:
            project_dir = scenario_project_dir(temp_root, scenario, len(scenarios))
            ok = run_scenario(repo_root, project_dir, scenario) and ok
        ok = run_template_adapter_smoke(repo_root) and ok
        return 0 if ok else 1
    finally:
        if args.keep:
            print(f"kept external smoke project: {temp_root}")
        elif temp_context is not None:
            temp_context.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())
