use std::path::Path;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, EdgeReconnectable, Graph, GraphId,
    GraphOp, GraphTransaction, Group, GroupId, Node, NodeExtent, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphPanInertiaTuning, NodeGraphViewState};
use jellyflow_runtime::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceFixtureDirectory,
    ConformanceFixtureDirectoryApprovalReport, ConformanceFixtureDirectoryReport,
    ConformanceNodeDragSessionContract, ConformanceRunReport, ConformanceScenario,
    ConformanceSuite, ConformanceSuiteReport, ConformanceTraceConfig, ConformanceTraceEvent,
    ConformanceViewChange, ConformanceViewportDragPanSessionContract,
};
use jellyflow_runtime::runtime::connection::{ConnectionHandleRef, ConnectionHandleValidity};
use jellyflow_runtime::runtime::delete::DELETE_SELECTION_TRANSACTION_LABEL;
use jellyflow_runtime::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use jellyflow_runtime::runtime::events::{
    NodeGraphGestureEvent, NodeResizeEnd, NodeResizeEndOutcome, NodeResizeStart, NodeResizeUpdate,
};
use jellyflow_runtime::runtime::geometry::{HandleBounds, HandlePosition};
use jellyflow_runtime::runtime::measurement::{MeasuredHandle, NodeMeasurement};
use jellyflow_runtime::runtime::resize::{
    NODE_RESIZE_TRANSACTION_LABEL, NodePointerResizeRequest, NodeResizeDirection, NodeResizeRequest,
};
use jellyflow_runtime::runtime::viewport::{
    ViewportAnimationEasing, ViewportAnimationOptions, ViewportAnimationPlan,
    ViewportAnimationRequest, ViewportDoubleClickZoomInput, ViewportDragPanInput,
    ViewportGestureContext, ViewportPanInertiaRequest, ViewportPanRequest, ViewportPointerButton,
    ViewportTransform, plan_viewport_pan_inertia,
};
use jellyflow_runtime::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_runtime::runtime::{store::NodeGraphStore, xyflow::ControlledGraph};

pub fn adapter_smoke_suite() -> ConformanceSuite {
    ConformanceSuite::new("headless adapter template").with_scenarios([
        node_drag_scenario(),
        node_drag_parent_expansion_scenario(),
        node_resize_scenario(),
        delete_selection_scenario(),
        viewport_pan_scenario(),
        viewport_constrained_pan_scenario(),
        visible_node_ids_scenario(),
        visible_node_render_order_scenario(),
        visible_edge_ids_scenario(),
        visible_edge_render_order_scenario(),
        viewport_animation_scenario(),
        viewport_pan_inertia_scenario(),
    ])
}

pub fn check_builtin_suite() -> ConformanceSuiteReport {
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

pub fn run_viewport_animation_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &viewport_animation_scenario(),
    )
    .map_err(|err| err.to_string())
}

pub fn run_visible_node_ids_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(&visible_node_ids_scenario())
        .map_err(|err| err.to_string())
}

pub fn run_visible_node_render_order_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &visible_node_render_order_scenario(),
    )
    .map_err(|err| err.to_string())
}

pub fn run_visible_edge_ids_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(&visible_edge_ids_scenario())
        .map_err(|err| err.to_string())
}

pub fn run_visible_edge_render_order_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &visible_edge_render_order_scenario(),
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
    let mut graph = graph_with_connected_nodes(source_id, target_id, out_port, in_port, edge_id);
    graph.nodes.get_mut(&source_id).expect("source exists").size = None;
    graph.nodes.get_mut(&target_id).expect("target exists").size = None;

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

    let query = store.rendering_query(viewport);
    if query.visible_node_ids != vec![source_id, target_id] {
        return Err(format!(
            "expected measured visible nodes {:?}, got {:?}",
            vec![source_id, target_id],
            query.visible_node_ids
        ));
    }
    if query.visible_edge_ids != vec![edge_id] {
        return Err(format!(
            "expected measured visible edge {:?}, got {:?}",
            edge_id, query.visible_edge_ids
        ));
    }

    let endpoints = store
        .edge_position_from_measurements(edge_id)
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

    let target = store.resolve_connection_target_from_measurements(
        CanvasPoint { x: 264.0, y: 60.0 },
        source_handle,
    );
    if target.feedback != ConnectionHandleValidity::Valid || !target.is_handle_valid {
        return Err(format!("expected valid measured target, got {target:?}"));
    }

    Ok(())
}

fn node_drag_scenario() -> ConformanceScenario {
    let node_id = NodeId::from_u128(2);
    let graph = graph_with_node(node_id);
    let start = CanvasPoint { x: 12.0, y: 16.0 };
    let target = CanvasPoint { x: 96.0, y: 128.0 };

    ConformanceScenario::new("template node drag", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_node_drag_session_contract(ConformanceNodeDragSessionContract::new(
            node_id, start, target,
        ))
}

fn node_drag_parent_expansion_scenario() -> ConformanceScenario {
    let node_id = NodeId::from_u128(3);
    let parent_id = GroupId::from_u128(30);
    let graph = graph_with_parent_expanding_node(node_id, parent_id);
    let target = CanvasPoint { x: 95.0, y: 95.0 };

    ConformanceScenario::new("template node drag parent expansion", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::apply_node_drag(node_id, target)])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(NODE_DRAG_TRANSACTION_LABEL),
                ["set_node_pos", "set_group_rect"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_DRAG_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ])
}

fn node_resize_scenario() -> ConformanceScenario {
    let node_id = NodeId::from_u128(4);
    let graph = graph_with_node(node_id);
    let direction = NodeResizeDirection::BottomRight;
    let start_pointer = CanvasPoint { x: 230.0, y: 140.0 };
    let current_pointer = CanvasPoint { x: 250.0, y: 150.0 };
    let start = NodeResizeStart {
        node: node_id,
        direction,
        pointer: start_pointer,
    };
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
    let end = NodeResizeEnd {
        node: node_id,
        direction,
        pointer: current_pointer,
        outcome: NodeResizeEndOutcome::Committed,
    };
    let start_event = NodeGraphGestureEvent::NodeResizeStart(start.clone());
    let update_event = NodeGraphGestureEvent::NodeResizeUpdate(update.clone());
    let end_event = NodeGraphGestureEvent::NodeResizeEnd(end.clone());

    ConformanceScenario::new("template node resize", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::apply_node_resize(
                NodeResizeRequest::new(
                    node_id,
                    CanvasSize {
                        width: 220.0,
                        height: 120.0,
                    },
                )
                .with_direction(NodeResizeDirection::BottomRight),
            ),
            ConformanceAction::apply_node_pointer_resize_session(NodePointerResizeRequest::new(
                node_id,
                start_pointer,
                current_pointer,
                direction,
            )),
        ])
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
            ConformanceTraceEvent::gesture(start_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeResizeStart(start)),
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
            ConformanceTraceEvent::gesture(update_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeResize(update)),
            ConformanceTraceEvent::gesture(end_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeResizeEnd(end)),
        ])
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
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::apply_delete_selection_for_key(
            keyboard_types::Code::Backspace,
        )])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(DELETE_SELECTION_TRANSACTION_LABEL),
                ["remove_node"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(DELETE_SELECTION_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 1,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::EdgesChange { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectionChange(
                ConnectionChange::Disconnected(disconnected),
            )),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::Disconnect(disconnected)),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesDelete { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::EdgesDelete { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::Delete {
                nodes: 1,
                edges: 1,
                groups: 0,
                sticky_notes: 0,
            }),
            ConformanceTraceEvent::selection(Vec::new(), Vec::new(), Vec::new()),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Selection {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    groups: Vec::new(),
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::SelectionChange {
                nodes: Vec::new(),
                edges: Vec::new(),
                groups: Vec::new(),
            }),
        ])
}

fn viewport_pan_scenario() -> ConformanceScenario {
    let graph = Graph::new(GraphId::from_u128(10));
    let pan = CanvasPoint { x: 40.0, y: -10.0 };
    let start = ViewportTransform::new(CanvasPoint::default(), 1.0).expect("valid viewport");
    let end = ViewportTransform::new(pan, 1.0).expect("valid viewport");

    ConformanceScenario::new("template viewport pan", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
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
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
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

fn visible_node_ids_scenario() -> ConformanceScenario {
    let inside = NodeId::from_u128(70);
    let partial = NodeId::from_u128(71);
    let outside = NodeId::from_u128(72);
    let mut graph = Graph::new(GraphId::from_u128(13));
    graph.nodes.insert(
        inside,
        template_node(
            CanvasPoint { x: 0.0, y: 0.0 },
            Vec::new(),
            Some(CanvasSize {
                width: 40.0,
                height: 40.0,
            }),
        ),
    );
    graph.nodes.insert(
        partial,
        template_node(
            CanvasPoint { x: 95.0, y: 95.0 },
            Vec::new(),
            Some(CanvasSize {
                width: 40.0,
                height: 40.0,
            }),
        ),
    );
    graph.nodes.insert(
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

    ConformanceScenario::new("template visible node ids", graph)
        .with_actions([ConformanceAction::assert_visible_node_ids(
            CanvasSize {
                width: 100.0,
                height: 100.0,
            },
            [inside, partial],
        )])
        .with_expected_trace([])
}

fn visible_node_render_order_scenario() -> ConformanceScenario {
    let (graph, view_state, selected, partial, _outside) = visible_node_render_order_fixture();

    ConformanceScenario::new("template visible node render order", graph)
        .with_view_state(view_state)
        .with_actions([ConformanceAction::assert_visible_node_render_order(
            CanvasSize {
                width: 100.0,
                height: 100.0,
            },
            [partial, selected],
        )])
        .with_expected_trace([])
}

fn visible_node_render_order_fixture() -> (Graph, NodeGraphViewState, NodeId, NodeId, NodeId) {
    let selected = NodeId::from_u128(73);
    let partial = NodeId::from_u128(74);
    let outside = NodeId::from_u128(75);
    let mut graph = Graph::new(GraphId::from_u128(14));
    graph.nodes.insert(
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
    graph.nodes.insert(
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
    graph.nodes.insert(
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

    (graph, view_state, selected, partial, outside)
}

fn visible_edge_ids_scenario() -> ConformanceScenario {
    let (graph, _view_state, visible_edge) = visible_edge_render_order_fixture();

    ConformanceScenario::new("template visible edge ids", graph)
        .with_actions([ConformanceAction::assert_visible_edge_ids(
            CanvasSize {
                width: 100.0,
                height: 100.0,
            },
            [visible_edge],
        )])
        .with_expected_trace([])
}

fn visible_edge_render_order_scenario() -> ConformanceScenario {
    let (graph, view_state, visible_edge) = visible_edge_render_order_fixture();

    ConformanceScenario::new("template visible edge render order", graph)
        .with_view_state(view_state)
        .with_actions([ConformanceAction::assert_visible_edge_render_order(
            CanvasSize {
                width: 100.0,
                height: 100.0,
            },
            [visible_edge],
        )])
        .with_expected_trace([])
}

fn visible_edge_render_order_fixture() -> (Graph, NodeGraphViewState, EdgeId) {
    let source_id = NodeId::from_u128(76);
    let target_id = NodeId::from_u128(77);
    let out_port = PortId::from_u128(78);
    let in_port = PortId::from_u128(79);
    let edge_id = EdgeId::from_u128(80);
    let mut graph = graph_with_connected_nodes(source_id, target_id, out_port, in_port, edge_id);
    let source = graph.nodes.get_mut(&source_id).expect("source node exists");
    source.pos = CanvasPoint { x: -80.0, y: 0.0 };
    source.size = Some(CanvasSize {
        width: 40.0,
        height: 40.0,
    });
    let target = graph.nodes.get_mut(&target_id).expect("target node exists");
    target.pos = CanvasPoint { x: 140.0, y: 0.0 };
    target.size = Some(CanvasSize {
        width: 40.0,
        height: 40.0,
    });
    let mut view_state = NodeGraphViewState {
        edge_draw_order: vec![edge_id],
        ..NodeGraphViewState::default()
    };
    view_state.set_selection(Vec::new(), vec![edge_id], Vec::new());

    (graph, view_state, edge_id)
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
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
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
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
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
    let mut graph = Graph::new(GraphId::from_u128(1));
    graph.nodes.insert(
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
    let mut graph = Graph::new(GraphId::from_u128(2));
    graph.nodes.insert(
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
    graph.nodes.insert(
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
    graph.ports.insert(
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
    graph.ports.insert(
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
    graph.edges.insert(
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
    let mut graph = graph_with_node(node_id);
    graph.groups.insert(
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
    let node = graph.nodes.get_mut(&node_id).expect("node exists");
    node.parent = Some(parent_id);
    node.extent = Some(NodeExtent::Parent);
    node.expand_parent = Some(true);
    node.size = Some(CanvasSize {
        width: 20.0,
        height: 20.0,
    });
    graph
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn built_in_headless_suite_matches() {
        let report = check_builtin_suite();

        assert!(report.is_match(), "{report}");
        assert_eq!(report.scenario_count(), 12);
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
    fn viewport_animation_smoke_runs_as_single_scenario() {
        let report = run_viewport_animation_smoke().expect("viewport animation scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn visible_node_ids_smoke_runs_as_single_scenario() {
        let report = run_visible_node_ids_smoke().expect("visible node ids scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn visible_node_render_order_smoke_runs_as_single_scenario() {
        let report =
            run_visible_node_render_order_smoke().expect("visible node render order scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn visible_edge_ids_smoke_runs_as_single_scenario() {
        let report = run_visible_edge_ids_smoke().expect("visible edge ids scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn visible_edge_render_order_smoke_runs_as_single_scenario() {
        let report =
            run_visible_edge_render_order_smoke().expect("visible edge render order scenario runs");

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
        assert_eq!(report.scenario_count(), 12);
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
