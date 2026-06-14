use std::collections::BTreeMap;
use std::path::Path;

use jellyflow_core::{
    Binding, BindingId, CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind,
    EdgeReconnectable, Graph, GraphBuilder, GraphId, GraphOp, GraphTransaction, Group, GroupId,
    Node, NodeExtent, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey,
    PortKind,
};
use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphPanInertiaTuning, NodeGraphViewState};
use jellyflow_runtime::runtime::binding::BindingEndpointResolutionStatus;
use jellyflow_runtime::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceDeleteSelectionContract,
    ConformanceDeleteSelectionDuringNodeDragContract, ConformanceEdgeEndpointPosition,
    ConformanceFixtureDirectory, ConformanceFixtureDirectoryApprovalReport,
    ConformanceFixtureDirectoryReport, ConformanceLayoutEdgePosition,
    ConformanceLayoutFactsConnectionTargetExpectation, ConformanceLayoutFactsContract,
    ConformanceLayoutFactsExpectation, ConformanceNodeDragSessionContract,
    ConformanceNodeResizeSessionContract, ConformanceRenderingQueryContract, ConformanceRunReport,
    ConformanceScenario, ConformanceSuite, ConformanceSuiteReport, ConformanceTraceEvent,
    ConformanceViewChange, ConformanceViewportDragPanSessionContract,
};
use jellyflow_runtime::runtime::connection::{
    ConnectionHandleConnection, ConnectionHandleRef, ConnectionHandleValidity,
    ConnectionTargetHandle, ResolvedConnectionTarget,
};
use jellyflow_runtime::runtime::events::{
    NodeDragEnd, NodeDragEndOutcome, NodeDragStart, NodeResizeUpdate,
};
use jellyflow_runtime::runtime::geometry::{HandleBounds, HandlePosition};
use jellyflow_runtime::runtime::layout::{LayoutFamilyId, builtin_layout_engine_registry};
use jellyflow_runtime::runtime::measurement::{MeasuredHandle, NodeMeasurement};
use jellyflow_runtime::runtime::rendering::RenderingQueryResult;
use jellyflow_runtime::runtime::resize::{
    NODE_RESIZE_TRANSACTION_LABEL, NodePointerResizeRequest, NodeResizeDirection, NodeResizeRequest,
};
use jellyflow_runtime::runtime::viewport::{
    ViewportAnimationEasing, ViewportAnimationOptions, ViewportAnimationPlan,
    ViewportAnimationRequest, ViewportDoubleClickZoomInput, ViewportDragPanInput,
    ViewportGestureContext, ViewportPanInertiaRequest, ViewportPanRequest, ViewportPointerButton,
    ViewportTransform, plan_viewport_pan_inertia,
};
use jellyflow_runtime::runtime::xyflow::callbacks::EdgeConnection;
use jellyflow_runtime::runtime::{store::NodeGraphStore, xyflow::ControlledGraph};
use jellyflow_runtime::schema::{NodeKindViewDescriptor, NodeRegistry, NodeSchema, PortDecl};

pub fn adapter_smoke_suite() -> ConformanceSuite {
    ConformanceSuite::new("headless adapter template").with_scenarios([
        node_drag_scenario(),
        node_drag_parent_expansion_scenario(),
        node_resize_scenario(),
        layout_facts_scenario(),
        delete_selection_scenario(),
        delete_during_active_drag_scenario(),
        viewport_pan_scenario(),
        viewport_constrained_pan_scenario(),
        rendering_query_contract_scenario(),
        viewport_animation_scenario(),
        viewport_pan_inertia_scenario(),
    ])
}

pub fn check_builtin_suite() -> ConformanceSuiteReport {
    assert_knowledge_canvas_surfaces();
    run_create_node_palette_smoke().expect("create-node palette smoke runs");
    run_custom_node_renderer_registry_smoke().expect("custom node renderer registry smoke runs");
    adapter_smoke_suite().run()
}

pub fn check_fixture_directory(
    fixture_dir: impl AsRef<Path>,
) -> Result<ConformanceFixtureDirectoryReport, String> {
    let directory = ConformanceFixtureDirectory::load_json(fixture_dir.as_ref())
        .map_err(|err| err.to_string())?;
    Ok(directory.run())
}

pub fn approve_fixture_directory(
    fixture_dir: impl AsRef<Path>,
) -> Result<ConformanceFixtureDirectoryApprovalReport, String> {
    let directory = ConformanceFixtureDirectory::load_json(fixture_dir.as_ref())
        .map_err(|err| err.to_string())?;
    directory
        .approve_actual_traces_to_json()
        .map_err(|err| err.to_string())
}

pub fn run_node_drag_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(&node_drag_scenario())
        .map_err(|err| err.to_string())
}

pub fn run_node_drag_parent_expansion_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &node_drag_parent_expansion_scenario(),
    )
    .map_err(|err| err.to_string())
}

pub fn run_node_resize_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(&node_resize_scenario())
        .map_err(|err| err.to_string())
}

pub fn run_delete_selection_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(&delete_selection_scenario())
        .map_err(|err| err.to_string())
}

pub fn run_delete_during_active_drag_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &delete_during_active_drag_scenario(),
    )
    .map_err(|err| err.to_string())
}

pub fn run_viewport_animation_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &viewport_animation_scenario(),
    )
    .map_err(|err| err.to_string())
}

pub fn run_rendering_query_contract_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &rendering_query_contract_scenario(),
    )
    .map_err(|err| err.to_string())
}

pub fn run_viewport_constrained_pan_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &viewport_constrained_pan_scenario(),
    )
    .map_err(|err| err.to_string())
}

pub fn run_viewport_pan_inertia_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &viewport_pan_inertia_scenario(),
    )
    .map_err(|err| err.to_string())
}

pub fn run_controlled_graph_smoke() -> Result<(), String> {
    let source_id = NodeId::from_u128(80);
    let target_id = NodeId::from_u128(81);
    let out_port = PortId::from_u128(82);
    let in_port = PortId::from_u128(83);
    let edge_id = EdgeId::from_u128(84);
    let graph = graph_with_connected_nodes(source_id, target_id, out_port, in_port, edge_id);
    let mut store = NodeGraphStore::new(
        graph.clone(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let mut controlled = ControlledGraph::new(graph);
    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodePos {
            id: source_id,
            from: CanvasPoint { x: 10.0, y: 20.0 },
            to: CanvasPoint { x: 42.0, y: 64.0 },
        },
        GraphOp::SetEdgeReconnectable {
            id: edge_id,
            from: None,
            to: Some(EdgeReconnectable::Bool(false)),
        },
    ]);

    let outcome = store
        .dispatch_transaction(&tx)
        .map_err(|err| err.to_string())?;
    let report = controlled.apply_patch_changes(&outcome.patch);
    if report.applied() != 2 || report.ignored() != 0 {
        return Err(format!(
            "expected controlled patch to apply 2 changes and ignore 0, got applied={} ignored={}",
            report.applied(),
            report.ignored()
        ));
    }

    let store_graph = serde_json::to_value(store.graph()).map_err(|err| err.to_string())?;
    let controlled_graph =
        serde_json::to_value(controlled.graph()).map_err(|err| err.to_string())?;
    if controlled_graph != store_graph {
        return Err("controlled graph diverged from store graph".to_owned());
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterNodeRenderer {
    pub component: &'static str,
    pub expects_source_binding: bool,
}

impl AdapterNodeRenderer {
    pub const fn note_card() -> Self {
        Self {
            component: "NoteCard",
            expects_source_binding: true,
        }
    }
}

#[derive(Debug, Default)]
pub struct AdapterRendererRegistry {
    node_renderers: BTreeMap<String, AdapterNodeRenderer>,
}

impl AdapterRendererRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtin_nodes() -> Self {
        let mut registry = Self::new();
        registry.register_node_renderer("note-card", AdapterNodeRenderer::note_card());
        registry
    }

    pub fn register_node_renderer(
        &mut self,
        renderer_key: impl Into<String>,
        renderer: AdapterNodeRenderer,
    ) -> &mut Self {
        self.node_renderers.insert(renderer_key.into(), renderer);
        self
    }

    pub fn renderer_for_key(&self, renderer_key: &str) -> Option<&AdapterNodeRenderer> {
        self.node_renderers.get(renderer_key)
    }

    pub fn renderer_for_descriptor(
        &self,
        descriptor: &NodeKindViewDescriptor,
    ) -> Option<&AdapterNodeRenderer> {
        self.renderer_for_key(&descriptor.renderer_key)
    }
}

pub fn template_node_registry() -> NodeRegistry {
    let mut registry = NodeRegistry::new();
    registry.register(template_note_schema());
    registry
}

pub fn template_note_schema() -> NodeSchema {
    NodeSchema {
        kind: NodeKindKey::new("template.note"),
        latest_kind_version: 1,
        kind_aliases: vec![NodeKindKey::new("template.sticky")],
        title: "Note".to_owned(),
        category: vec!["Knowledge".to_owned()],
        keywords: vec!["memo".to_owned()],
        renderer_key: Some("note-card".to_owned()),
        default_size: Some(CanvasSize {
            width: 160.0,
            height: 96.0,
        }),
        ports: vec![
            PortDecl {
                key: PortKey::new("source"),
                dir: PortDirection::In,
                kind: PortKind::Data,
                capacity: PortCapacity::Single,
                ty: None,
                label: Some("Source".to_owned()),
            },
            PortDecl {
                key: PortKey::new("result"),
                dir: PortDirection::Out,
                kind: PortKind::Data,
                capacity: PortCapacity::Multi,
                ty: None,
                label: Some("Result".to_owned()),
            },
        ],
        default_data: serde_json::json!({ "body": "" }),
    }
}

pub fn run_custom_node_renderer_registry_smoke() -> Result<(), String> {
    let registry = template_node_registry();
    let descriptors = registry.view_descriptors();
    let descriptor = descriptors
        .first()
        .ok_or_else(|| "expected at least one node descriptor".to_owned())?;

    let renderers = AdapterRendererRegistry::with_builtin_nodes();
    let renderer = renderers
        .renderer_for_descriptor(descriptor)
        .ok_or_else(|| format!("missing renderer for key {}", descriptor.renderer_key))?;
    if *renderer != AdapterNodeRenderer::note_card() {
        return Err(format!("expected NoteCard renderer, got {renderer:?}"));
    }

    let mut dynamic_renderers = AdapterRendererRegistry::new();
    if dynamic_renderers
        .renderer_for_descriptor(descriptor)
        .is_some()
    {
        return Err("empty adapter renderer registry unexpectedly resolved descriptor".to_owned());
    }
    dynamic_renderers.register_node_renderer(
        descriptor.renderer_key.clone(),
        AdapterNodeRenderer::note_card(),
    );
    if dynamic_renderers.renderer_for_descriptor(descriptor)
        != Some(&AdapterNodeRenderer::note_card())
    {
        return Err("dynamic renderer registration did not resolve descriptor key".to_owned());
    }

    Ok(())
}

pub fn run_create_node_palette_smoke() -> Result<(), String> {
    let registry = template_node_registry();

    let descriptors = registry.view_descriptors();
    if descriptors.len() != 1 || descriptors[0].renderer_key != "note-card" {
        return Err(format!(
            "expected one note-card descriptor, got {descriptors:?}"
        ));
    }

    let mut store = NodeGraphStore::new(
        Graph::new(GraphId::from_u128(16)),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let outcome = store
        .apply_create_node_from_schema(
            &registry,
            jellyflow_runtime::runtime::create_node::CreateNodeRequest::new(
                NodeKindKey::new("template.sticky"),
                CanvasPoint { x: 32.0, y: 48.0 },
            ),
        )
        .map_err(|err| err.to_string())?;
    let node_id = outcome.node_id();
    let port_ids: Vec<_> = outcome.port_ids().collect();
    let node = store
        .graph()
        .nodes()
        .get(&node_id)
        .ok_or_else(|| "created node missing from store graph".to_owned())?;

    if node.kind != NodeKindKey::new("template.note") {
        return Err(format!("expected canonical note kind, got {:?}", node.kind));
    }
    if node.ports != port_ids || port_ids.len() != 2 {
        return Err(format!(
            "expected two ordered ports {:?}, got node ports {:?}",
            port_ids, node.ports
        ));
    }
    if outcome.dispatch.committed().label()
        != Some(jellyflow_runtime::runtime::create_node::CREATE_NODE_TRANSACTION_LABEL)
    {
        return Err(format!(
            "expected create-node transaction label, got {:?}",
            outcome.dispatch.committed().label()
        ));
    }

    Ok(())
}

pub fn run_rendering_query_smoke() -> Result<(), String> {
    let (graph, view_state, selected, partial, outside) = visible_node_render_order_fixture();
    let store = NodeGraphStore::new(graph, view_state, NodeGraphEditorConfig::default());
    let result = store.rendering_query(CanvasSize {
        width: 100.0,
        height: 100.0,
    });

    let expected_node_order = vec![outside, partial, selected];
    if result.node_order != expected_node_order {
        return Err(format!(
            "expected node order {expected_node_order:?}, got {:?}",
            result.node_order
        ));
    }

    let expected_visible_node_ids = vec![selected, partial];
    if result.visible_node_ids != expected_visible_node_ids {
        return Err(format!(
            "expected visible node ids {expected_visible_node_ids:?}, got {:?}",
            result.visible_node_ids
        ));
    }

    let expected_visible_render_order = vec![partial, selected];
    if result.visible_node_render_order != expected_visible_render_order {
        return Err(format!(
            "expected visible render order {expected_visible_render_order:?}, got {:?}",
            result.visible_node_render_order
        ));
    }

    let (edge_graph, edge_view_state, visible_edge) = visible_edge_render_order_fixture();
    let edge_store = NodeGraphStore::new(
        edge_graph,
        edge_view_state,
        NodeGraphEditorConfig::default(),
    );
    let edge_result = edge_store.rendering_query(CanvasSize {
        width: 100.0,
        height: 100.0,
    });
    if edge_result.visible_edge_ids != vec![visible_edge] {
        return Err(format!(
            "expected visible edge ids {:?}, got {:?}",
            vec![visible_edge],
            edge_result.visible_edge_ids
        ));
    }
    if edge_result.visible_edge_render_order != vec![visible_edge] {
        return Err(format!(
            "expected visible edge render order {:?}, got {:?}",
            vec![visible_edge],
            edge_result.visible_edge_render_order
        ));
    }

    Ok(())
}

pub fn run_measurement_smoke() -> Result<(), String> {
    let source_id = NodeId::from_u128(90);
    let target_id = NodeId::from_u128(91);
    let out_port = PortId::from_u128(92);
    let in_port = PortId::from_u128(93);
    let edge_id = EdgeId::from_u128(94);
    let mut graph = connected_nodes_builder(source_id, target_id, out_port, in_port, edge_id);
    graph
        .update_node(&source_id, |node| node.size = None)
        .expect("source exists");
    graph
        .update_node(&target_id, |node| node.size = None)
        .expect("target exists");
    let graph = graph.build_unchecked();

    let mut store = NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let viewport = CanvasSize {
        width: 300.0,
        height: 100.0,
    };
    if !store.rendering_query(viewport).visible_node_ids.is_empty() {
        return Err("unmeasured nodes should not participate in visible-node culling".to_owned());
    }

    let source_handle = ConnectionHandleRef::new(source_id, out_port, PortDirection::Out);
    let target_handle = ConnectionHandleRef::new(target_id, in_port, PortDirection::In);
    store
        .report_node_measurement(
            NodeMeasurement::new(source_id)
                .with_size(Some(CanvasSize {
                    width: 160.0,
                    height: 80.0,
                }))
                .with_handles([MeasuredHandle::new(
                    source_handle,
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 152.0, y: 32.0 },
                            size: CanvasSize {
                                width: 8.0,
                                height: 16.0,
                            },
                        },
                        position: HandlePosition::Right,
                    },
                )]),
        )
        .map_err(|err| err.to_string())?;
    store
        .report_node_measurement(
            NodeMeasurement::new(target_id)
                .with_size(Some(CanvasSize {
                    width: 160.0,
                    height: 80.0,
                }))
                .with_handles([MeasuredHandle::new(
                    target_handle,
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 0.0, y: 32.0 },
                            size: CanvasSize {
                                width: 8.0,
                                height: 16.0,
                            },
                        },
                        position: HandlePosition::Left,
                    },
                )]),
        )
        .map_err(|err| err.to_string())?;

    let facts = store.layout_facts_query(viewport);
    if facts.rendering.visible_node_ids != vec![source_id, target_id] {
        return Err(format!(
            "expected measured visible nodes {:?}, got {:?}",
            vec![source_id, target_id],
            facts.rendering.visible_node_ids
        ));
    }
    if facts.rendering.visible_edge_ids != vec![edge_id] {
        return Err(format!(
            "expected measured visible edge {:?}, got {:?}",
            edge_id, facts.rendering.visible_edge_ids
        ));
    }

    let endpoints = facts
        .visible_edge_position(edge_id)
        .ok_or_else(|| "expected measured edge endpoints".to_owned())?;
    if endpoints.source.point != (CanvasPoint { x: 170.0, y: 60.0 }) {
        return Err(format!(
            "expected source endpoint at (170, 60), got {:?}",
            endpoints.source.point
        ));
    }
    if endpoints.target.point != (CanvasPoint { x: 260.0, y: 60.0 }) {
        return Err(format!(
            "expected target endpoint at (260, 60), got {:?}",
            endpoints.target.point
        ));
    }

    let target = store.resolve_connection_target_from_layout_facts(
        CanvasPoint { x: 264.0, y: 60.0 },
        source_handle,
    );
    if target.feedback != ConnectionHandleValidity::Valid || !target.is_handle_valid {
        return Err(format!("expected valid measured target, got {target:?}"));
    }

    Ok(())
}

fn assert_knowledge_canvas_surfaces() {
    let node_id = NodeId::from_u128(1);
    let binding_id = BindingId::from_u128(2);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(
        node_id,
        Node {
            kind: NodeKindKey::new("knowledge.note"),
            kind_version: 1,
            pos: CanvasPoint { x: 0.0, y: 0.0 },
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
                width: 120.0,
                height: 72.0,
            }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::json!({ "title": "Knowledge note" }),
        },
    );
    graph.insert_binding(
        binding_id,
        Binding::node_to_source(
            node_id,
            "source://paper.pdf",
            serde_json::json!({ "page": 1 }),
        )
        .with_kind("excerpt"),
    );

    let store = NodeGraphStore::new(
        graph.build_unchecked(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    assert_eq!(
        store
            .binding_query()
            .binding(binding_id)
            .expect("binding reachable")
            .subject
            .status(),
        BindingEndpointResolutionStatus::Resolved
    );
    assert_eq!(
        builtin_layout_engine_registry()
            .engines_for_family(&LayoutFamilyId::mind_map())
            .count(),
        2
    );
}

fn node_drag_scenario() -> ConformanceScenario {
    let node_id = NodeId::from_u128(2);
    let graph = graph_with_node(node_id);
    let start = CanvasPoint { x: 12.0, y: 16.0 };
    let target = CanvasPoint { x: 96.0, y: 128.0 };

    ConformanceScenario::new("template node drag", graph)
        .with_xyflow_callbacks()
        .with_node_drag_session_contract(ConformanceNodeDragSessionContract::new(
            node_id, start, target,
        ))
}

fn node_drag_parent_expansion_scenario() -> ConformanceScenario {
    let node_id = NodeId::from_u128(3);
    let parent_id = GroupId::from_u128(30);
    let graph = graph_with_parent_expanding_node(node_id, parent_id);
    let start = CanvasPoint { x: 50.0, y: 50.0 };
    let target = CanvasPoint { x: 95.0, y: 95.0 };

    ConformanceScenario::new("template node drag parent expansion", graph)
        .with_xyflow_callbacks()
        .with_node_drag_session_contract(
            ConformanceNodeDragSessionContract::new(node_id, start, target)
                .with_commit_op_kinds(["set_node_pos", "set_group_rect"]),
        )
}

fn node_resize_scenario() -> ConformanceScenario {
    let node_id = NodeId::from_u128(4);
    let graph = graph_with_node(node_id);
    let direction = NodeResizeDirection::BottomRight;
    let start_pointer = CanvasPoint { x: 230.0, y: 140.0 };
    let current_pointer = CanvasPoint { x: 250.0, y: 150.0 };
    let session_request =
        NodePointerResizeRequest::new(node_id, start_pointer, current_pointer, direction);
    let update = NodeResizeUpdate {
        node: node_id,
        direction,
        pointer: current_pointer,
        position: CanvasPoint { x: 10.0, y: 20.0 },
        size: CanvasSize {
            width: 240.0,
            height: 130.0,
        },
    };

    ConformanceScenario::new("template node resize", graph)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::apply_node_resize(
            NodeResizeRequest::new(
                node_id,
                CanvasSize {
                    width: 220.0,
                    height: 120.0,
                },
            )
            .with_direction(NodeResizeDirection::BottomRight),
        )])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(NODE_RESIZE_TRANSACTION_LABEL),
                ["set_node_size"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_RESIZE_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ])
        .with_node_resize_session_contract(ConformanceNodeResizeSessionContract::new(
            session_request,
            update,
        ))
}

fn layout_facts_scenario() -> ConformanceScenario {
    let source_id = NodeId::from_u128(90);
    let target_id = NodeId::from_u128(91);
    let out_port = PortId::from_u128(92);
    let in_port = PortId::from_u128(93);
    let edge_id = EdgeId::from_u128(94);
    let mut graph = connected_nodes_builder(source_id, target_id, out_port, in_port, edge_id);
    graph
        .update_node(&source_id, |node| node.size = None)
        .expect("source exists");
    graph
        .update_node(&target_id, |node| node.size = None)
        .expect("target exists");
    let graph = graph.build_unchecked();

    let source_handle = ConnectionHandleRef::new(source_id, out_port, PortDirection::Out);
    let target_handle = ConnectionHandleRef::new(target_id, in_port, PortDirection::In);
    let source_measurement = NodeMeasurement::new(source_id)
        .with_size(Some(CanvasSize {
            width: 160.0,
            height: 80.0,
        }))
        .with_handles([MeasuredHandle::new(
            source_handle,
            HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 152.0, y: 32.0 },
                    size: CanvasSize {
                        width: 8.0,
                        height: 16.0,
                    },
                },
                position: HandlePosition::Right,
            },
        )]);
    let target_measurement = NodeMeasurement::new(target_id)
        .with_size(Some(CanvasSize {
            width: 160.0,
            height: 80.0,
        }))
        .with_handles([MeasuredHandle::new(
            target_handle,
            HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 0.0, y: 32.0 },
                    size: CanvasSize {
                        width: 8.0,
                        height: 16.0,
                    },
                },
                position: HandlePosition::Left,
            },
        )]);
    let expected_target_handle = ConnectionTargetHandle::new(target_handle, true, true);
    let expected_target = ResolvedConnectionTarget {
        target: Some(expected_target_handle),
        connection: Some(ConnectionHandleConnection {
            source: source_handle,
            target: target_handle,
        }),
        is_handle_valid: true,
        feedback: ConnectionHandleValidity::Valid,
    };
    let expected = ConformanceLayoutFactsExpectation::new([source_id, target_id], [edge_id])
        .with_edge_positions([ConformanceLayoutEdgePosition::new(
            edge_id,
            ConformanceEdgeEndpointPosition::new(
                CanvasPoint { x: 170.0, y: 60.0 },
                HandlePosition::Right,
            ),
            ConformanceEdgeEndpointPosition::new(
                CanvasPoint { x: 260.0, y: 60.0 },
                HandlePosition::Left,
            ),
        )])
        .with_connection_target(ConformanceLayoutFactsConnectionTargetExpectation::new(
            CanvasPoint { x: 264.0, y: 60.0 },
            source_handle,
            expected_target,
        ));

    ConformanceScenario::new("template layout facts", graph).with_layout_facts_contract(
        ConformanceLayoutFactsContract::new(
            [source_measurement, target_measurement],
            CanvasSize {
                width: 300.0,
                height: 100.0,
            },
            expected,
        ),
    )
}

fn delete_selection_scenario() -> ConformanceScenario {
    let node_id = NodeId::from_u128(5);
    let sibling_id = NodeId::from_u128(6);
    let out_port = PortId::from_u128(50);
    let in_port = PortId::from_u128(60);
    let edge_id = EdgeId::from_u128(500);
    let graph = graph_with_connected_nodes(node_id, sibling_id, out_port, in_port, edge_id);
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![node_id], vec![edge_id], Vec::new());
    let disconnected = EdgeConnection::new(edge_id, out_port, in_port, EdgeKind::Data);

    ConformanceScenario::new("template delete selection", graph)
        .with_view_state(view_state)
        .with_xyflow_callbacks()
        .with_delete_selection_contract(
            ConformanceDeleteSelectionContract::new(1, 1)
                .for_key(keyboard_types::Code::Backspace)
                .with_disconnected([disconnected]),
        )
}

fn delete_during_active_drag_scenario() -> ConformanceScenario {
    let node_id = NodeId::from_u128(7);
    let sibling_id = NodeId::from_u128(8);
    let out_port = PortId::from_u128(70);
    let in_port = PortId::from_u128(80);
    let edge_id = EdgeId::from_u128(700);
    let graph = graph_with_connected_nodes(node_id, sibling_id, out_port, in_port, edge_id);
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![node_id], Vec::new(), Vec::new());
    let disconnected = EdgeConnection::new(edge_id, out_port, in_port, EdgeKind::Data);
    let start = NodeDragStart {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
    };
    let end = NodeDragEnd {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
        outcome: NodeDragEndOutcome::Canceled,
    };

    ConformanceScenario::new("template delete during active node drag", graph)
        .with_view_state(view_state)
        .with_xyflow_callbacks()
        .with_delete_selection_during_node_drag_contract(
            ConformanceDeleteSelectionDuringNodeDragContract::new(
                start,
                end,
                ConformanceDeleteSelectionContract::new(1, 1)
                    .for_key(keyboard_types::Code::Backspace)
                    .with_disconnected([disconnected]),
            ),
        )
}

fn viewport_pan_scenario() -> ConformanceScenario {
    let graph = Graph::new(GraphId::from_u128(10));
    let pan = CanvasPoint { x: 40.0, y: -10.0 };
    let start = ViewportTransform::new(CanvasPoint::default(), 1.0).expect("valid viewport");
    let end = ViewportTransform::new(pan, 1.0).expect("valid viewport");

    ConformanceScenario::new("template viewport pan", graph)
        .with_xyflow_callbacks()
        .with_viewport_drag_pan_session_contract(ConformanceViewportDragPanSessionContract::new(
            ViewportGestureContext::idle(),
            ViewportDragPanInput::new(ViewportPointerButton::Left, pan),
            start,
            end,
        ))
}

fn viewport_constrained_pan_scenario() -> ConformanceScenario {
    let graph = Graph::new(GraphId::from_u128(15));
    let mut editor_config = NodeGraphEditorConfig::default();
    editor_config.interaction.translate_extent = Some(CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 100.0,
            height: 100.0,
        },
    });
    let requested_pan = CanvasPoint {
        x: 400.0,
        y: -300.0,
    };
    let constrained_pan = CanvasPoint { x: 0.0, y: -50.0 };

    ConformanceScenario::new("template viewport constrained pan", graph)
        .with_editor_config(editor_config)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::apply_viewport_pan_constrained(
            ViewportPanRequest::new(requested_pan),
            CanvasSize {
                width: 50.0,
                height: 50.0,
            },
        )])
        .with_expected_trace([
            ConformanceTraceEvent::viewport(constrained_pan, 1.0),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport {
                    pan: constrained_pan,
                    zoom: 1.0,
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan: constrained_pan,
                zoom: 1.0,
            }),
        ])
}

fn rendering_query_contract_scenario() -> ConformanceScenario {
    let (graph, view_state, selected, partial, outside, edge_id) =
        rendering_query_contract_fixture();
    let viewport_size = CanvasSize {
        width: 100.0,
        height: 100.0,
    };
    let expected = RenderingQueryResult {
        group_order: Vec::new(),
        node_order: vec![outside, partial, selected],
        edge_order: vec![edge_id],
        visible_node_ids: vec![selected, partial],
        visible_node_render_order: vec![partial, selected],
        visible_edge_ids: vec![edge_id],
        visible_edge_render_order: vec![edge_id],
    };

    ConformanceScenario::new("template rendering query", graph)
        .with_view_state(view_state)
        .with_rendering_query_contract(ConformanceRenderingQueryContract::new(
            viewport_size,
            expected,
        ))
}

fn rendering_query_contract_fixture() -> (Graph, NodeGraphViewState, NodeId, NodeId, NodeId, EdgeId)
{
    let selected = NodeId::from_u128(73);
    let partial = NodeId::from_u128(74);
    let outside = NodeId::from_u128(75);
    let out_port = PortId::from_u128(78);
    let in_port = PortId::from_u128(79);
    let edge_id = EdgeId::from_u128(80);
    let mut graph = connected_nodes_builder(selected, partial, out_port, in_port, edge_id);

    graph
        .update_node(&selected, |node| {
            node.pos = CanvasPoint { x: 0.0, y: 0.0 };
            node.size = Some(CanvasSize {
                width: 40.0,
                height: 40.0,
            });
        })
        .expect("selected node exists");
    graph
        .update_node(&partial, |node| {
            node.pos = CanvasPoint { x: 95.0, y: 0.0 };
            node.size = Some(CanvasSize {
                width: 40.0,
                height: 40.0,
            });
        })
        .expect("partial node exists");
    graph.insert_node(
        outside,
        template_node(
            CanvasPoint { x: 180.0, y: 0.0 },
            Vec::new(),
            Some(CanvasSize {
                width: 40.0,
                height: 40.0,
            }),
        ),
    );

    let mut view_state = NodeGraphViewState {
        draw_order: vec![outside, selected, partial],
        edge_draw_order: vec![edge_id],
        ..NodeGraphViewState::default()
    };
    view_state.set_selection(vec![selected], vec![edge_id], Vec::new());

    (
        graph.build_unchecked(),
        view_state,
        selected,
        partial,
        outside,
        edge_id,
    )
}

fn visible_node_render_order_fixture() -> (Graph, NodeGraphViewState, NodeId, NodeId, NodeId) {
    let selected = NodeId::from_u128(73);
    let partial = NodeId::from_u128(74);
    let outside = NodeId::from_u128(75);
    let mut graph = GraphBuilder::new(GraphId::from_u128(14));
    graph.insert_node(
        selected,
        template_node(
            CanvasPoint { x: 0.0, y: 0.0 },
            Vec::new(),
            Some(CanvasSize {
                width: 40.0,
                height: 40.0,
            }),
        ),
    );
    graph.insert_node(
        partial,
        template_node(
            CanvasPoint { x: 95.0, y: 0.0 },
            Vec::new(),
            Some(CanvasSize {
                width: 40.0,
                height: 40.0,
            }),
        ),
    );
    graph.insert_node(
        outside,
        template_node(
            CanvasPoint { x: 180.0, y: 0.0 },
            Vec::new(),
            Some(CanvasSize {
                width: 40.0,
                height: 40.0,
            }),
        ),
    );
    let mut view_state = NodeGraphViewState {
        draw_order: vec![outside, selected, partial],
        ..NodeGraphViewState::default()
    };
    view_state.set_selection(vec![selected], Vec::new(), Vec::new());

    (graph.build_unchecked(), view_state, selected, partial, outside)
}

fn visible_edge_render_order_fixture() -> (Graph, NodeGraphViewState, EdgeId) {
    let source_id = NodeId::from_u128(76);
    let target_id = NodeId::from_u128(77);
    let out_port = PortId::from_u128(78);
    let in_port = PortId::from_u128(79);
    let edge_id = EdgeId::from_u128(80);
    let mut graph = connected_nodes_builder(source_id, target_id, out_port, in_port, edge_id);
    graph
        .update_node(&source_id, |node| {
            node.pos = CanvasPoint { x: -80.0, y: 0.0 };
            node.size = Some(CanvasSize {
                width: 40.0,
                height: 40.0,
            });
        })
        .expect("source node exists");
    graph
        .update_node(&target_id, |node| {
            node.pos = CanvasPoint { x: 140.0, y: 0.0 };
            node.size = Some(CanvasSize {
                width: 40.0,
                height: 40.0,
            });
        })
        .expect("target node exists");
    let mut view_state = NodeGraphViewState {
        edge_draw_order: vec![edge_id],
        ..NodeGraphViewState::default()
    };
    view_state.set_selection(Vec::new(), vec![edge_id], Vec::new());

    (graph.build_unchecked(), view_state, edge_id)
}

fn viewport_animation_scenario() -> ConformanceScenario {
    let graph = Graph::new(GraphId::from_u128(11));
    let from = ViewportTransform::new(CanvasPoint { x: 0.0, y: 0.0 }, 1.0).expect("valid viewport");
    let to =
        ViewportTransform::new(CanvasPoint { x: 80.0, y: -40.0 }, 2.0).expect("valid viewport");
    let midpoint_pan = CanvasPoint { x: 40.0, y: -20.0 };
    let endpoint_pan = CanvasPoint { x: 80.0, y: -40.0 };

    let double_click_current =
        ViewportTransform::new(CanvasPoint { x: 10.0, y: 20.0 }, 2.0).expect("valid viewport");
    let double_click_target =
        ViewportTransform::new(CanvasPoint { x: -10.0, y: 10.0 }, 3.0).expect("valid viewport");
    let expected_plan = ViewportAnimationPlan {
        from: double_click_current,
        to: double_click_target,
        duration_seconds: 0.2,
        easing: ViewportAnimationEasing::CubicInOut,
    };

    ConformanceScenario::new("template viewport animation", graph)
        .with_xyflow_callbacks()
        .with_actions([
            ConformanceAction::apply_viewport_animation_frames(
                ViewportAnimationRequest::new(from, to, ViewportAnimationOptions::new(1.0)),
                [0.5, 1.0],
            ),
            ConformanceAction::assert_viewport_double_click_zoom(
                ViewportDoubleClickZoomInput::new(
                    double_click_current,
                    CanvasPoint { x: 120.0, y: 60.0 },
                    2.0,
                    0.5,
                    3.0,
                    ViewportAnimationOptions::new(0.2),
                ),
                expected_plan,
            ),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::viewport(midpoint_pan, 1.5),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport {
                    pan: midpoint_pan,
                    zoom: 1.5,
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan: midpoint_pan,
                zoom: 1.5,
            }),
            ConformanceTraceEvent::viewport(endpoint_pan, 2.0),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport {
                    pan: endpoint_pan,
                    zoom: 2.0,
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan: endpoint_pan,
                zoom: 2.0,
            }),
        ])
}

fn viewport_pan_inertia_scenario() -> ConformanceScenario {
    let graph = Graph::new(GraphId::from_u128(12));
    let tuning = NodeGraphPanInertiaTuning {
        enabled: true,
        decay_per_s: 2.0,
        min_speed: 100.0,
        max_speed: 1000.0,
    };
    let request = ViewportPanInertiaRequest::new(
        ViewportTransform::new(CanvasPoint::default(), 2.0).expect("valid viewport"),
        CanvasPoint { x: 1000.0, y: 0.0 },
        tuning.clone(),
    );
    let plan = plan_viewport_pan_inertia(request.clone()).expect("inertia plan");
    let mid = plan.frame_at(0.5).expect("mid inertia frame");
    let terminal = plan.terminal_frame().expect("terminal inertia frame");

    ConformanceScenario::new("template viewport pan inertia", graph)
        .with_xyflow_callbacks()
        .with_actions([
            ConformanceAction::apply_viewport_pan_inertia_frames(
                request,
                [0.5, plan.duration_seconds],
            ),
            ConformanceAction::expect_viewport_pan_inertia_rejected(
                ViewportPanInertiaRequest::new(
                    ViewportTransform::new(CanvasPoint::default(), 1.0).expect("valid viewport"),
                    CanvasPoint { x: 50.0, y: 0.0 },
                    tuning,
                ),
            ),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::viewport(mid.transform.pan, mid.transform.zoom),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport {
                    pan: mid.transform.pan,
                    zoom: mid.transform.zoom,
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan: mid.transform.pan,
                zoom: mid.transform.zoom,
            }),
            ConformanceTraceEvent::viewport(terminal.transform.pan, terminal.transform.zoom),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport {
                    pan: terminal.transform.pan,
                    zoom: terminal.transform.zoom,
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan: terminal.transform.pan,
                zoom: terminal.transform.zoom,
            }),
        ])
}

fn graph_with_node(node_id: NodeId) -> Graph {
    node_builder(node_id).build_unchecked()
}

fn node_builder(node_id: NodeId) -> GraphBuilder {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(
        node_id,
        template_node(
            CanvasPoint { x: 10.0, y: 20.0 },
            Vec::new(),
            Some(CanvasSize {
                width: 160.0,
                height: 80.0,
            }),
        ),
    );
    graph
}

fn graph_with_connected_nodes(
    source_id: NodeId,
    target_id: NodeId,
    out_port: PortId,
    in_port: PortId,
    edge_id: EdgeId,
) -> Graph {
    connected_nodes_builder(source_id, target_id, out_port, in_port, edge_id).build_unchecked()
}

fn connected_nodes_builder(
    source_id: NodeId,
    target_id: NodeId,
    out_port: PortId,
    in_port: PortId,
    edge_id: EdgeId,
) -> GraphBuilder {
    let mut graph = GraphBuilder::new(GraphId::from_u128(2));
    graph.insert_node(
        source_id,
        template_node(
            CanvasPoint { x: 10.0, y: 20.0 },
            vec![out_port],
            Some(CanvasSize {
                width: 160.0,
                height: 80.0,
            }),
        ),
    );
    graph.insert_node(
        target_id,
        template_node(
            CanvasPoint { x: 260.0, y: 20.0 },
            vec![in_port],
            Some(CanvasSize {
                width: 160.0,
                height: 80.0,
            }),
        ),
    );
    graph.insert_port(
        out_port,
        Port {
            node: source_id,
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
    );
    graph.insert_port(
        in_port,
        Port {
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
    );
    graph.insert_edge(
        edge_id,
        Edge {
            kind: EdgeKind::Data,
            from: out_port,
            to: in_port,
            hidden: false,
            selectable: None,
            focusable: None,
            interaction_width: None,
            deletable: None,
            reconnectable: None,
        },
    );
    graph
}

fn template_node(pos: CanvasPoint, ports: Vec<PortId>, size: Option<CanvasSize>) -> Node {
    Node {
        kind: NodeKindKey::new("template.node"),
        kind_version: 1,
        pos,
        origin: None,
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size,
        hidden: false,
        collapsed: false,
        ports,
        data: serde_json::Value::Null,
    }
}

fn graph_with_parent_expanding_node(node_id: NodeId, parent_id: GroupId) -> Graph {
    let mut graph = node_builder(node_id);
    graph.insert_group(
        parent_id,
        Group {
            title: "Parent".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 100.0,
                },
            },
            color: None,
        },
    );
    graph
        .update_node(&node_id, |node| {
            node.parent = Some(parent_id);
            node.extent = Some(NodeExtent::Parent);
            node.expand_parent = Some(true);
            node.size = Some(CanvasSize {
                width: 20.0,
                height: 20.0,
            });
        })
        .expect("node exists");
    graph.build_unchecked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn built_in_headless_suite_matches() {
        let report = check_builtin_suite();

        assert!(report.is_match(), "{report}");
        assert_eq!(report.scenario_count(), 11);
    }

    #[test]
    fn knowledge_canvas_surfaces_are_reachable() {
        assert_knowledge_canvas_surfaces();
    }

    #[test]
    fn node_drag_smoke_runs_as_single_scenario() {
        let report = run_node_drag_smoke().expect("node drag scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn node_drag_parent_expansion_smoke_runs_as_single_scenario() {
        let report = run_node_drag_parent_expansion_smoke()
            .expect("node drag parent expansion scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn node_resize_smoke_runs_as_single_scenario() {
        let report = run_node_resize_smoke().expect("node resize scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn delete_selection_smoke_runs_as_single_scenario() {
        let report = run_delete_selection_smoke().expect("delete selection scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn delete_during_active_drag_smoke_runs_as_single_scenario() {
        let report =
            run_delete_during_active_drag_smoke().expect("delete during active drag scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn viewport_animation_smoke_runs_as_single_scenario() {
        let report = run_viewport_animation_smoke().expect("viewport animation scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn rendering_query_contract_smoke_runs_as_single_scenario() {
        let report =
            run_rendering_query_contract_smoke().expect("rendering query contract scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn viewport_constrained_pan_smoke_runs_as_single_scenario() {
        let report =
            run_viewport_constrained_pan_smoke().expect("viewport constrained pan scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn viewport_pan_inertia_smoke_runs_as_single_scenario() {
        let report = run_viewport_pan_inertia_smoke().expect("viewport pan inertia scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn controlled_graph_smoke_applies_store_patch() {
        run_controlled_graph_smoke().expect("controlled graph smoke runs");
    }

    #[test]
    fn create_node_palette_smoke_uses_schema_registry() {
        run_create_node_palette_smoke().expect("create-node palette smoke runs");
    }

    #[test]
    fn custom_node_renderer_registry_maps_schema_descriptors() {
        run_custom_node_renderer_registry_smoke()
            .expect("custom node renderer registry smoke runs");
    }

    #[test]
    fn rendering_query_smoke_resolves_order_and_visibility() {
        run_rendering_query_smoke().expect("rendering query smoke runs");
    }

    #[test]
    fn measurement_smoke_resolves_runtime_layout_facts() {
        run_measurement_smoke().expect("measurement smoke runs");
    }

    #[test]
    fn saved_suite_can_be_checked_as_fixture_directory() {
        let root = temp_fixture_dir("roundtrip");
        std::fs::create_dir_all(&root).expect("create fixture directory");
        adapter_smoke_suite()
            .save_json(root.join("suite.json"))
            .expect("save fixture suite");

        let report = check_fixture_directory(&root).expect("check fixture directory");
        let _ = std::fs::remove_dir_all(&root);

        assert!(report.is_match(), "{report}");
        assert_eq!(report.file_count(), 1);
        assert_eq!(report.scenario_count(), 11);
    }

    fn temp_fixture_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "jellyflow-headless-adapter-template-{name}-{nanos}"
        ))
    }
}
