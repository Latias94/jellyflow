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
    return [
        jellyflow_facade_scenario(repo_root),
        jellyflow_runtime_scenario(repo_root),
        jellyflow_egui_scenario(repo_root),
    ]


def jellyflow_facade_scenario(repo_root: Path) -> SmokeScenario:
    return SmokeScenario(
        slug="jellyflow-facade-external-smoke",
        cargo_toml=jellyflow_facade_cargo_toml(repo_root),
        main_rs=jellyflow_facade_main_rs(),
    )


def jellyflow_facade_cargo_toml(repo_root: Path) -> str:
    return (
        textwrap.dedent(
            f"""
            [package]
            name = "jellyflow-facade-external-smoke"
            version = "0.0.0"
            edition = "2024"
            rust-version = "1.95"
            publish = false

            [dependencies]
            jellyflow = {{ path = "{(repo_root / "crates/jellyflow").as_posix()}" }}
            """
        ).strip()
        + "\n"
    )


def jellyflow_facade_main_rs() -> str:
    return (
        textwrap.dedent(
            """
            use jellyflow::prelude::*;
            use jellyflow::{core, layout, runtime};

            fn main() {
                let store = NodeGraphStore::new(
                    Graph::new(GraphId::from_u128(1)),
                    NodeGraphViewState::default(),
                    NodeGraphEditorConfig::default(),
                );

                assert_eq!(store.graph().nodes().len(), 0);

                let registry = builtin_layout_engine_registry();
                assert!(registry.get(&LayoutEngineId::dugong()).is_some());
                assert!(
                    registry
                        .engines_for_family(&LayoutFamilyId::mind_map())
                        .count()
                        >= 2
                );

                let request = LayoutEngineRequest::dugong(LayoutRequest::all());
                let planned = store
                    .plan_layout(&request, &registry)
                    .expect("facade layout planning succeeds");
                assert!(planned.nodes.is_empty());

                let _graph: core::Graph = store.graph().clone();
                let _patch = runtime::NodeGraphPatch::default();
                let _: fn() -> &'static layout::LayoutEngineRegistry =
                    layout::builtin_layout_engine_registry;
                let _ = std::mem::size_of::<DispatchOutcome>();
                let _ = std::mem::size_of::<DispatchError>();
            }
            """
        ).strip()
        + "\n"
    )


def jellyflow_runtime_scenario(repo_root: Path) -> SmokeScenario:
    return SmokeScenario(
        slug="jellyflow-external-smoke",
        cargo_toml=jellyflow_runtime_cargo_toml(repo_root),
        main_rs=jellyflow_runtime_main_rs(),
    )


def jellyflow_egui_scenario(repo_root: Path) -> SmokeScenario:
    return SmokeScenario(
        slug="jellyflow-egui-external-smoke",
        cargo_toml=jellyflow_egui_cargo_toml(repo_root),
        main_rs=jellyflow_egui_main_rs(),
    )


def jellyflow_egui_cargo_toml(repo_root: Path) -> str:
    return (
        textwrap.dedent(
            f"""
            [package]
            name = "jellyflow-egui-external-smoke"
            version = "0.0.0"
            edition = "2024"
            rust-version = "1.95"
            publish = false

            [dependencies]
            jellyflow-egui = {{ path = "{(repo_root / "crates/jellyflow-egui").as_posix()}" }}
            """
        ).strip()
        + "\n"
    )


def jellyflow_egui_main_rs() -> str:
    return (
        textwrap.dedent(
            """
            use jellyflow_egui::{
                JellyflowEguiApp, JellyflowEguiBridge, NodeRendererStyle, RendererCatalog,
                SampleGraphKind, egui,
            };

            fn main() {
                let app = JellyflowEguiApp::demo().expect("demo app builds");
                assert!(!app.bridge.store().graph().nodes().is_empty());

                let mut catalog = RendererCatalog::new();
                catalog.register("custom-card", NodeRendererStyle::fallback());
                assert_eq!(
                    catalog.style_for_key("custom-card"),
                    NodeRendererStyle::fallback()
                );

                let bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
                assert!(bridge.descriptors().len() >= 4);
                for sample in [
                    SampleGraphKind::AutomationBuilder,
                    SampleGraphKind::Erd,
                    SampleGraphKind::OrgChart,
                ] {
                    let app = JellyflowEguiApp::sample(sample).expect("product sample app builds");
                    assert!(!app.bridge.store().graph().nodes().is_empty());
                    assert!(!app.bridge.store().graph().edges().is_empty());
                }
                let _ = egui::Color32::WHITE;
            }
            """
        ).strip()
        + "\n"
    )


def jellyflow_runtime_cargo_toml(repo_root: Path) -> str:
    return (
        textwrap.dedent(
            f"""
            [package]
            name = "jellyflow-external-smoke"
            version = "0.0.0"
            edition = "2024"
            rust-version = "1.95"
            publish = false

            [dependencies]
            jellyflow-core = {{ path = "{(repo_root / "crates/jellyflow-core").as_posix()}" }}
            jellyflow-layout = {{ path = "{(repo_root / "crates/jellyflow-layout").as_posix()}" }}
            jellyflow-runtime = {{ path = "{(repo_root / "crates/jellyflow-runtime").as_posix()}" }}
            serde_json = "1"
            """
        ).strip()
        + "\n"
    )


def jellyflow_runtime_main_rs() -> str:
    return (
        textwrap.dedent(
            """
            use jellyflow_core::{
                Binding, BindingEndpoint, BindingId, CanvasPoint, CanvasRect, CanvasSize, Edge,
                EdgeId, EdgeKind, EdgeLabelAnchor, EdgeViewDescriptor, Graph, GraphElementKeys,
                GraphElements, GraphId, GraphLocalBindingTarget, GraphOp, GraphTransaction, Node,
                NodeGraphModifierKey, NodeGraphModifiers, NodeId, NodeKindKey, Port,
                PortCapacity, PortDirection, PortId, PortKey, PortKind, SourceAnchor,
            };
            use jellyflow_layout::{
                LayoutContext, LayoutEngineId, LayoutEngineRequest, LayoutFamilyId,
                LayoutRequest, LayoutScope, builtin_layout_engine_registry,
                layout_graph_with_engine,
            };
            use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
            use jellyflow_runtime::profile::{
                ConnectionRuleDescriptor, FieldSchema, GraphProfileMetadata, NodeFieldSchemaSet,
                ValidationHint, VariableDescriptor, VariableSurfaceDescriptor,
            };
            use jellyflow_runtime::rules::{Diagnostic, DiagnosticTarget};
            use jellyflow_runtime::schema::{
                NodeRegistry, NodeSchema, PortDecl, PortHandleVisibility, PortViewDescriptor,
                PortViewSide,
            };
            use jellyflow_runtime::runtime::binding::BindingEndpointResolutionStatus;
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
                let mut registry = NodeRegistry::new();
                registry.register(
                    NodeSchema::builder("demo.workflow", "Workflow")
                        .port(
                            PortDecl::data_input("input").with_view(
                                PortViewDescriptor::left()
                                    .with_order(0)
                                    .with_group("data")
                                    .with_anchor("field.input")
                                    .with_visibility(PortHandleVisibility::Visible),
                            ),
                        )
                        .port(PortDecl::data_output("result").on_right().with_view_order(1))
                        .build(),
                );
                let workflow_descriptor = registry
                    .view_descriptor(&NodeKindKey::new("demo.workflow"))
                    .expect("workflow descriptor");
                assert_eq!(
                    workflow_descriptor.ports[0].view.side,
                    Some(PortViewSide::Left)
                );
                assert_eq!(
                    workflow_descriptor.ports[0].view.anchor.as_deref(),
                    Some("field.input")
                );

                let mut add = GraphTransaction::new().with_label("add demo node");
                add.push(GraphOp::AddNode { id: node_id, node });
                add.apply_to(&mut graph).expect("transaction applies");
                assert!(graph.nodes().contains_key(&node_id));
                let binding_id = BindingId::from_u128(40);
                let mut binding_tx = GraphTransaction::new().with_label("add binding");
                binding_tx.push(GraphOp::AddBinding {
                    id: binding_id,
                    binding: Binding {
                        subject: BindingEndpoint::graph_local(
                            GraphLocalBindingTarget::Node { id: node_id },
                        ),
                        target: BindingEndpoint::source(SourceAnchor::new(
                            "source://paper.pdf",
                            serde_json::json!({ "page": 1, "rect": [10, 20, 30, 40] }),
                        )),
                        kind: Some("excerpt".to_owned()),
                        meta: serde_json::Value::Null,
                    },
                });
                binding_tx.apply_to(&mut graph).expect("binding applies");
                let target_id = NodeId::from_u128(3);
                let from_port = PortId::from_u128(50);
                let to_port = PortId::from_u128(51);
                let edge_id = EdgeId::from_u128(52);
                let edge_view = EdgeViewDescriptor::new()
                    .with_renderer_key("branch-edge")
                    .with_label("approved")
                    .with_label_anchor(EdgeLabelAnchor::Center);

                let target_node = make_node("demo.target", 220.0, 20.0);
                let mut edge_tx = GraphTransaction::new().with_label("add edge metadata");
                edge_tx.push(GraphOp::AddNode {
                    id: target_id,
                    node: target_node,
                });
                edge_tx.push(GraphOp::AddPort {
                    id: from_port,
                    port: Port {
                        node: node_id,
                        key: PortKey::new("out"),
                        dir: PortDirection::Out,
                        kind: PortKind::Data,
                        capacity: PortCapacity::Multi,
                        connectable: None,
                        connectable_start: None,
                        connectable_end: None,
                        ty: None,
                        data: serde_json::Value::Null,
                    },
                });
                edge_tx.push(GraphOp::AddPort {
                    id: to_port,
                    port: Port {
                        node: target_id,
                        key: PortKey::new("in"),
                        dir: PortDirection::In,
                        kind: PortKind::Data,
                        capacity: PortCapacity::Single,
                        connectable: None,
                        connectable_start: None,
                        connectable_end: None,
                        ty: None,
                        data: serde_json::Value::Null,
                    },
                });
                edge_tx.push(GraphOp::SetNodePorts {
                    id: node_id,
                    from: Vec::new(),
                    to: vec![from_port],
                });
                edge_tx.push(GraphOp::SetNodePorts {
                    id: target_id,
                    from: Vec::new(),
                    to: vec![to_port],
                });
                edge_tx.push(GraphOp::AddEdge {
                    id: edge_id,
                    edge: Edge::new(EdgeKind::Data, from_port, to_port),
                });
                edge_tx.push(GraphOp::SetEdgeData {
                    id: edge_id,
                    from: serde_json::Value::Null,
                    to: serde_json::json!({ "branch": "approved" }),
                });
                edge_tx.push(GraphOp::SetEdgeView {
                    id: edge_id,
                    from: EdgeViewDescriptor::default(),
                    to: edge_view.clone(),
                });
                edge_tx.apply_to(&mut graph).expect("edge metadata applies");
                assert_eq!(
                    graph.edges()[&edge_id].data,
                    serde_json::json!({ "branch": "approved" })
                );
                assert_eq!(graph.edges()[&edge_id].view, edge_view);
                let profile_metadata =
                    GraphProfileMetadata::new("external.workflow", "External workflow")
                        .with_node_fields(
                            NodeFieldSchemaSet::new("demo.workflow").with_field(
                                FieldSchema::new("prompt", "Prompt")
                                    .with_type(jellyflow_core::TypeDesc::String)
                                    .required()
                                    .with_hint(ValidationHint::new(
                                        "external.prompt_required",
                                        "Prompt is required",
                                    ))
                                    .with_port_anchor("input"),
                            ),
                        )
                        .with_variable_surface(
                            VariableSurfaceDescriptor::new("inputs", "Inputs").with_variable(
                                VariableDescriptor::new("topic", "Topic")
                                    .with_type(jellyflow_core::TypeDesc::String),
                            ),
                        )
                        .with_connection_rule(
                            ConnectionRuleDescriptor::new(
                                "external.exec_dag",
                                "Execution flow must be acyclic",
                            )
                            .for_edge_kind(EdgeKind::Exec),
                        );
                assert_eq!(profile_metadata.node_fields[0].fields[0].key, "prompt");
                let diagnostic = Diagnostic::error(
                    "external.edge.invalid",
                    DiagnosticTarget::Edge { id: edge_id },
                    "edge is invalid",
                );
                assert_eq!(diagnostic.key, "external.edge.invalid");

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
                let graph_nodes: GraphElements<'_, NodeId, Node> = store.graph().nodes();
                let graph_node_keys: GraphElementKeys<'_, NodeId, Node> = graph_nodes.keys();
                assert_eq!(graph_node_keys.count(), graph_nodes.len());
                assert_eq!(outcome.committed().ops.len(), 1);
                assert_eq!(outcome.patch().transaction().ops.len(), 1);
                assert!(outcome.footprint().nodes.contains(&node_id));
                assert_eq!(outcome.footprint(), outcome.patch().footprint());
                assert_eq!(
                    LayoutScope::from_footprint(store.graph(), outcome.footprint()).nodes(),
                    Some(&[node_id].into_iter().collect())
                );
                let owned_patch = outcome.clone().into_patch();
                assert_eq!(owned_patch.transaction().ops.len(), 1);
                assert_eq!(
                    store.graph().nodes()[&node_id].pos,
                    CanvasPoint { x: 32.0, y: 48.0 }
                );

                let layout_registry = builtin_layout_engine_registry();
                assert_eq!(
                    layout_registry
                        .engines_for_family(&LayoutFamilyId::mind_map())
                        .count(),
                    2
                );
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
                assert_eq!(
                    store
                        .binding_query()
                        .binding(binding_id)
                        .expect("binding query result")
                        .subject
                        .status(),
                    BindingEndpointResolutionStatus::Resolved
                );
                assert!(
                    store
                        .layout_context_with_binding_pins()
                        .pinned_nodes
                        .contains(&node_id)
                );
                assert!(store.graph().edges().contains_key(&edge_id));

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
            "consume jellyflow and the lower-level Jellyflow crates without fret-node or fret-core."
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
