use crate::io::{NodeGraphEditorConfig, NodeGraphViewState};
use crate::runtime::connection::{ConnectionHandleRef, ConnectionHandleValidity};
use crate::runtime::geometry::{HandleBounds, HandlePosition};
use crate::runtime::measurement::{
    LayoutEdgePosition, MeasuredHandle, MeasuredSurfaceAnchor, MeasuredSurfaceSlot,
    NodeHandleMeasurementSource, NodeInternalsInvalidation, NodeInternalsInvalidationReason,
    NodeMeasurement, NodeMeasurementError, NodeMeasurementOutcome, NodeMeasurementStatus,
};
use crate::runtime::store::NodeGraphStore;
use crate::schema::NodeSurfaceSlotVisibility;
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, EdgeLabelAnchor, EdgeRouteKind,
    EdgeViewDescriptor, Graph, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn measured_size_feeds_rendering_query_without_persisting_graph_size() {
    let node = NodeId::from_u128(1);
    let graph = graph_with_unsized_node(node);
    let mut store = NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let viewport = CanvasSize {
        width: 10.0,
        height: 10.0,
    };
    let persisted_before = serde_json::to_value(store.graph()).expect("serialize graph");

    assert!(store.rendering_query(viewport).visible_node_ids.is_empty());

    let outcome = store
        .report_node_measurement(NodeMeasurement::new(node).with_size(Some(CanvasSize {
            width: 10.0,
            height: 10.0,
        })))
        .expect("node measurement");
    assert_eq!(outcome, NodeMeasurementOutcome::Changed);
    assert_eq!(store.rendering_query(viewport).visible_node_ids, vec![node]);
    assert_eq!(
        store.graph().nodes().get(&node).expect("node exists").size,
        None,
        "runtime measurements must not persist into Graph"
    );
    assert_eq!(
        serde_json::to_value(store.graph()).expect("serialize measured graph"),
        persisted_before,
        "runtime measurements must not change the persisted Graph payload"
    );

    assert_eq!(
        store.clear_node_measurement(node),
        NodeMeasurementOutcome::Changed
    );
    assert!(store.rendering_query(viewport).visible_node_ids.is_empty());
    assert_eq!(
        serde_json::to_value(store.graph()).expect("serialize cleared graph"),
        persisted_before,
        "clearing runtime measurements must not change the persisted Graph payload"
    );
}

#[test]
fn measured_handles_feed_edge_endpoints_and_connection_targets() {
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let out = PortId::from_u128(20);
    let input = PortId::from_u128(21);
    let edge = EdgeId::from_u128(30);
    let mut store = NodeGraphStore::new(
        graph_with_unsized_connected_nodes(source, target, out, input, edge),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let source_handle = ConnectionHandleRef::new(source, out, PortDirection::Out);
    let target_handle = ConnectionHandleRef::new(target, input, PortDirection::In);

    store
        .report_node_measurement(
            NodeMeasurement::new(source)
                .with_size(Some(CanvasSize {
                    width: 100.0,
                    height: 100.0,
                }))
                .with_handles([MeasuredHandle::new(
                    source_handle,
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 90.0, y: 40.0 },
                            size: CanvasSize {
                                width: 10.0,
                                height: 20.0,
                            },
                        },
                        position: HandlePosition::Right,
                    },
                )]),
        )
        .expect("source measurement");
    store
        .report_node_measurement(
            NodeMeasurement::new(target)
                .with_revision(0)
                .with_size(Some(CanvasSize {
                    width: 100.0,
                    height: 100.0,
                }))
                .with_handles([MeasuredHandle::new(
                    target_handle,
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 0.0, y: 40.0 },
                            size: CanvasSize {
                                width: 10.0,
                                height: 20.0,
                            },
                        },
                        position: HandlePosition::Left,
                    },
                )]),
        )
        .expect("target measurement");

    let endpoints = store
        .edge_position_from_layout_facts(edge)
        .expect("edge endpoints");
    assert_eq!(endpoints.source.point, CanvasPoint { x: 100.0, y: 50.0 });
    assert_eq!(endpoints.target.point, CanvasPoint { x: 200.0, y: 50.0 });

    let facts = store.layout_facts_query(CanvasSize {
        width: 320.0,
        height: 160.0,
    });
    assert_eq!(facts.revision, store.layout_facts_revision());
    assert_eq!(facts.rendering.visible_node_ids, vec![source, target]);
    assert_eq!(
        facts.visible_edge_positions,
        vec![LayoutEdgePosition::new(edge, endpoints)]
    );
    assert_eq!(facts.visible_edge_position(edge), Some(endpoints));
    assert!(
        facts.visible_edge_route_facts(edge).is_some(),
        "layout facts should include a renderer-neutral route fact for visible edges"
    );
    assert!(
        facts
            .connection_target_candidates
            .iter()
            .any(|candidate| candidate.target.handle == target_handle)
    );

    let target = store.resolve_connection_target_from_layout_facts(
        CanvasPoint { x: 205.0, y: 50.0 },
        source_handle,
    );
    assert_eq!(target.target.expect("target handle").handle, target_handle);
    assert_eq!(target.feedback, ConnectionHandleValidity::Valid);
    assert!(target.is_handle_valid);
}

#[test]
fn measured_semantic_anchor_feeds_edge_endpoints_and_connection_targets() {
    let source = NodeId::from_u128(60);
    let target = NodeId::from_u128(61);
    let out = PortId::from_u128(62);
    let input = PortId::from_u128(63);
    let edge = EdgeId::from_u128(64);
    let mut store = NodeGraphStore::new(
        graph_with_unsized_connected_nodes(source, target, out, input, edge),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let source_handle = ConnectionHandleRef::new(source, out, PortDirection::Out);
    let target_handle = ConnectionHandleRef::new(target, input, PortDirection::In);

    store
        .report_node_measurement(
            NodeMeasurement::new(source)
                .with_revision(1)
                .with_size(Some(CanvasSize {
                    width: 100.0,
                    height: 100.0,
                }))
                .with_handles([MeasuredHandle::new(
                    source_handle,
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 90.0, y: 40.0 },
                            size: CanvasSize {
                                width: 10.0,
                                height: 20.0,
                            },
                        },
                        position: HandlePosition::Right,
                    },
                )]),
        )
        .expect("source measurement");
    store
        .report_node_measurement(
            NodeMeasurement::new(target)
                .with_revision(7)
                .with_size(Some(CanvasSize {
                    width: 120.0,
                    height: 120.0,
                }))
                .with_slots([MeasuredSurfaceSlot::new(
                    "field.prompt",
                    CanvasRect {
                        origin: CanvasPoint { x: 8.0, y: 44.0 },
                        size: CanvasSize {
                            width: 104.0,
                            height: 24.0,
                        },
                    },
                )])
                .with_anchors([MeasuredSurfaceAnchor::new(
                    "field.prompt.input",
                    CanvasRect {
                        origin: CanvasPoint { x: 0.0, y: 48.0 },
                        size: CanvasSize {
                            width: 10.0,
                            height: 20.0,
                        },
                    },
                    HandlePosition::Left,
                )
                .with_port_key("p")]),
        )
        .expect("target measurement");

    let resolution = store.resolve_node_handle_measurement(target_handle);
    assert!(matches!(
        resolution.source,
        NodeHandleMeasurementSource::MeasuredAnchor { ref anchor }
            if anchor == "field.prompt.input"
    ));
    assert_eq!(
        resolution.bounds.expect("anchor bounds").rect.origin,
        CanvasPoint { x: 0.0, y: 48.0 }
    );

    let endpoints = store
        .edge_position_from_layout_facts(edge)
        .expect("edge endpoints");
    assert_eq!(endpoints.source.point, CanvasPoint { x: 100.0, y: 50.0 });
    assert_eq!(endpoints.target.point, CanvasPoint { x: 200.0, y: 58.0 });

    let facts = store.layout_facts_query(CanvasSize {
        width: 360.0,
        height: 180.0,
    });
    assert_eq!(
        facts.node_measurement_status(target),
        NodeMeasurementStatus::Fresh { revision: 7 }
    );
    assert!(
        facts
            .connection_target_candidates
            .iter()
            .any(|candidate| candidate.target.handle == target_handle
                && candidate.bounds.rect.origin == CanvasPoint { x: 0.0, y: 48.0 })
    );

    let target = store.resolve_connection_target_from_layout_facts(
        CanvasPoint { x: 205.0, y: 58.0 },
        source_handle,
    );
    assert_eq!(target.target.expect("target handle").handle, target_handle);
    assert_eq!(target.feedback, ConnectionHandleValidity::Valid);
}

#[test]
fn edge_route_facts_project_route_style_and_interaction_without_mutating_edge_data() {
    let source = NodeId::from_u128(120);
    let target = NodeId::from_u128(121);
    let out = PortId::from_u128(122);
    let input = PortId::from_u128(123);
    let edge = EdgeId::from_u128(124);
    let graph = graph_with_unsized_connected_nodes(source, target, out, input, edge);
    let mut view_state = NodeGraphViewState::default();
    view_state.selected_edges = vec![edge];
    let mut store = NodeGraphStore::new(graph, view_state, NodeGraphEditorConfig::default());
    let view = EdgeViewDescriptor::new()
        .with_label("approved")
        .with_label_anchor(EdgeLabelAnchor::Target)
        .with_route_kind(EdgeRouteKind::Straight)
        .with_hit_target_width(30.0);
    store
        .dispatch_transaction(&jellyflow_core::ops::GraphTransaction::from_ops([
            jellyflow_core::ops::GraphOp::SetEdgeView {
                id: edge,
                from: EdgeViewDescriptor::default(),
                to: view,
            },
        ]))
        .expect("set edge view");
    let graph_with_view = serde_json::to_value(store.graph()).expect("serialize graph with view");
    report_basic_connected_measurements(&mut store, source, target, out, input);

    let facts = store.layout_facts_query(CanvasSize {
        width: 360.0,
        height: 180.0,
    });
    let route = facts
        .visible_edge_route_facts(edge)
        .expect("visible edge route facts");

    assert_eq!(
        route.kind,
        crate::runtime::geometry::ResolvedEdgeRouteKind::Straight
    );
    assert_eq!(route.label_anchor, EdgeLabelAnchor::Target);
    assert_eq!(route.hit_test.interaction_width, 30.0);
    assert!(route.interaction.selected);
    assert!(route.interaction.selectable);
    assert!(route.interaction.can_reconnect());
    assert!(route.contains_point(CanvasPoint { x: 150.0, y: 55.0 }));
    assert!(
        !route.contains_point(CanvasPoint { x: 150.0, y: 66.0 }),
        "straight route should honor the configured interaction width"
    );
    assert_eq!(
        serde_json::to_value(store.graph()).expect("serialize after route facts"),
        graph_with_view,
        "route projection must not write ephemeral selection or route facts into the graph"
    );
}

#[test]
fn edge_route_facts_use_smoothstep_path_for_orthogonal_route_hint() {
    let source = NodeId::from_u128(130);
    let target = NodeId::from_u128(131);
    let out = PortId::from_u128(132);
    let input = PortId::from_u128(133);
    let edge = EdgeId::from_u128(134);
    let mut store = NodeGraphStore::new(
        graph_with_unsized_connected_nodes(source, target, out, input, edge),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    store
        .dispatch_transaction(&jellyflow_core::ops::GraphTransaction::from_ops([
            jellyflow_core::ops::GraphOp::SetEdgeView {
                id: edge,
                from: EdgeViewDescriptor::default(),
                to: EdgeViewDescriptor::new().with_route_kind(EdgeRouteKind::Orthogonal),
            },
        ]))
        .expect("set edge view");
    report_basic_connected_measurements(&mut store, source, target, out, input);

    let facts = store.layout_facts_query(CanvasSize {
        width: 360.0,
        height: 180.0,
    });
    let route = facts
        .visible_edge_route_facts(edge)
        .expect("visible edge route facts");

    assert_eq!(
        route.kind,
        crate::runtime::geometry::ResolvedEdgeRouteKind::Orthogonal
    );
    assert!(
        route.path.commands.len() > 2,
        "orthogonal route hint should survive projection as a multi-leg path"
    );
}

#[test]
fn invalidating_node_internals_marks_measurement_dirty_without_mutating_graph() {
    let source = NodeId::from_u128(70);
    let target = NodeId::from_u128(71);
    let out = PortId::from_u128(72);
    let input = PortId::from_u128(73);
    let edge = EdgeId::from_u128(74);
    let mut store = NodeGraphStore::new(
        graph_with_unsized_connected_nodes(source, target, out, input, edge),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let source_handle = ConnectionHandleRef::new(source, out, PortDirection::Out);
    let target_handle = ConnectionHandleRef::new(target, input, PortDirection::In);
    let persisted_before = serde_json::to_value(store.graph()).expect("serialize graph");

    report_basic_connected_measurements(&mut store, source, target, out, input);
    assert!(store.edge_position_from_layout_facts(edge).is_some());

    assert_eq!(
        store.invalidate_node_internals(NodeInternalsInvalidation::one(
            target,
            NodeInternalsInvalidationReason::DataChanged,
        )),
        NodeMeasurementOutcome::Changed
    );
    assert_eq!(
        store.node_measurement_status(target),
        NodeMeasurementStatus::Dirty {
            revision: 0,
            reason: NodeInternalsInvalidationReason::DataChanged,
        }
    );
    assert_eq!(
        serde_json::to_value(store.graph()).expect("serialize invalidated graph"),
        persisted_before,
        "invalidating internals must not mutate semantic graph data"
    );
    let dirty_endpoints = store
        .edge_position_from_layout_facts(edge)
        .expect("dirty measurements keep node-size fallback available");
    assert_eq!(
        dirty_endpoints.target.point,
        CanvasPoint { x: 200.0, y: 50.0 },
        "dirty target handle facts must not be reused for edge routing"
    );
    assert!(
        store
            .resolve_connection_target_from_layout_facts(
                CanvasPoint { x: 205.0, y: 50.0 },
                source_handle,
            )
            .target
            .is_none(),
        "dirty target handle facts must not be reused for connection hit tests"
    );

    store
        .report_node_measurement(
            NodeMeasurement::new(target)
                .with_revision(0)
                .with_size(Some(CanvasSize {
                    width: 100.0,
                    height: 100.0,
                }))
                .with_handles([MeasuredHandle::new(
                    target_handle,
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 0.0, y: 58.0 },
                            size: CanvasSize {
                                width: 10.0,
                                height: 20.0,
                            },
                        },
                        position: HandlePosition::Left,
                    },
                )]),
        )
        .expect("stale target measurement is structurally valid");
    assert_eq!(
        store.node_measurement_status(target),
        NodeMeasurementStatus::Dirty {
            revision: 0,
            reason: NodeInternalsInvalidationReason::DataChanged,
        },
        "measurements with the old revision must not clear dirty state"
    );

    store
        .report_node_measurement(
            NodeMeasurement::new(target)
                .with_revision(8)
                .with_size(Some(CanvasSize {
                    width: 100.0,
                    height: 100.0,
                }))
                .with_handles([MeasuredHandle::new(
                    target_handle,
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 0.0, y: 58.0 },
                            size: CanvasSize {
                                width: 10.0,
                                height: 20.0,
                            },
                        },
                        position: HandlePosition::Left,
                    },
                )]),
        )
        .expect("remeasured target");
    assert_eq!(
        store.node_measurement_status(target),
        NodeMeasurementStatus::Fresh { revision: 8 }
    );
    let endpoints = store
        .edge_position_from_layout_facts(edge)
        .expect("fresh endpoints");
    assert_eq!(endpoints.target.point, CanvasPoint { x: 200.0, y: 68.0 });
}

#[test]
fn node_data_transactions_invalidate_reported_internals_before_rerouting_edges() {
    let source = NodeId::from_u128(75);
    let target = NodeId::from_u128(76);
    let out = PortId::from_u128(77);
    let input = PortId::from_u128(78);
    let edge = EdgeId::from_u128(79);
    let mut store = NodeGraphStore::new(
        graph_with_unsized_connected_nodes(source, target, out, input, edge),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    report_basic_connected_measurements(&mut store, source, target, out, input);
    store
        .report_node_measurement(
            NodeMeasurement::new(target)
                .with_revision(7)
                .with_size(Some(CanvasSize {
                    width: 100.0,
                    height: 100.0,
                }))
                .with_handles([MeasuredHandle::new(
                    ConnectionHandleRef::new(target, input, PortDirection::In),
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 0.0, y: 16.0 },
                            size: CanvasSize {
                                width: 10.0,
                                height: 20.0,
                            },
                        },
                        position: HandlePosition::Left,
                    },
                )]),
        )
        .expect("custom target measurement");
    let before = store
        .edge_position_from_layout_facts(edge)
        .expect("fresh measured route");
    assert_eq!(before.target.point, CanvasPoint { x: 200.0, y: 26.0 });

    let from = store
        .graph()
        .nodes()
        .get(&target)
        .expect("target node")
        .data
        .clone();
    let to = serde_json::json!({ "fields": { "dynamic": "changed" } });
    store
        .dispatch_transaction(
            &GraphTransaction::from_ops([GraphOp::SetNodeData {
                id: target,
                from,
                to,
            }])
            .with_label("adapter data change"),
        )
        .expect("node data transaction");

    assert_eq!(
        store.node_measurement_status(target),
        NodeMeasurementStatus::Dirty {
            revision: 7,
            reason: NodeInternalsInvalidationReason::DataChanged,
        },
        "node data edits must mark adapter-reported internals stale"
    );
    let dirty = store
        .edge_position_from_layout_facts(edge)
        .expect("dirty route keeps node-size fallback");
    assert_eq!(
        dirty.target.point,
        CanvasPoint { x: 200.0, y: 50.0 },
        "stale target anchor must not keep routing after data changes"
    );

    store
        .report_node_measurement(
            NodeMeasurement::new(target)
                .with_revision(1)
                .with_size(Some(CanvasSize {
                    width: 100.0,
                    height: 100.0,
                }))
                .with_handles([MeasuredHandle::new(
                    ConnectionHandleRef::new(target, input, PortDirection::In),
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 0.0, y: 70.0 },
                            size: CanvasSize {
                                width: 10.0,
                                height: 20.0,
                            },
                        },
                        position: HandlePosition::Left,
                    },
                )]),
        )
        .expect("stale remeasurement is accepted but ignored");
    assert_eq!(
        store.node_measurement_status(target),
        NodeMeasurementStatus::Dirty {
            revision: 7,
            reason: NodeInternalsInvalidationReason::DataChanged,
        },
        "adapters must advance measurement revision before dirty internals become fresh again"
    );
    let stale = store
        .edge_position_from_layout_facts(edge)
        .expect("stale remeasurement still keeps fallback route");
    assert_eq!(stale.target.point, CanvasPoint { x: 200.0, y: 50.0 });

    store
        .report_node_measurement(
            NodeMeasurement::new(target)
                .with_revision(8)
                .with_size(Some(CanvasSize {
                    width: 100.0,
                    height: 100.0,
                }))
                .with_handles([MeasuredHandle::new(
                    ConnectionHandleRef::new(target, input, PortDirection::In),
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 0.0, y: 70.0 },
                            size: CanvasSize {
                                width: 10.0,
                                height: 20.0,
                            },
                        },
                        position: HandlePosition::Left,
                    },
                )]),
        )
        .expect("fresh remeasurement after data change");
    let fresh = store
        .edge_position_from_layout_facts(edge)
        .expect("fresh route after remeasurement");
    assert_eq!(fresh.target.point, CanvasPoint { x: 200.0, y: 80.0 });
}

#[test]
fn invalidating_multiple_nodes_keeps_unrelated_measurements_fresh() {
    let a = NodeId::from_u128(80);
    let b = NodeId::from_u128(81);
    let c = NodeId::from_u128(82);
    let mut graph = GraphBuilder::new(GraphId::from_u128(80));
    graph.insert_node(
        a,
        Node {
            pos: CanvasPoint::default(),
            ..node_fixture(Vec::new())
        },
    );
    graph.insert_node(
        b,
        Node {
            pos: CanvasPoint { x: 100.0, y: 0.0 },
            ..node_fixture(Vec::new())
        },
    );
    graph.insert_node(
        c,
        Node {
            pos: CanvasPoint { x: 200.0, y: 0.0 },
            ..node_fixture(Vec::new())
        },
    );
    let mut store = NodeGraphStore::new(
        graph.into(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    for (node, revision) in [(a, 1), (b, 2), (c, 3)] {
        store
            .report_node_measurement(
                NodeMeasurement::new(node)
                    .with_revision(revision)
                    .with_size(Some(CanvasSize {
                        width: 20.0,
                        height: 20.0,
                    })),
            )
            .expect("measurement");
    }

    assert_eq!(
        store.invalidate_node_internals(NodeInternalsInvalidation::new(
            [a, b],
            NodeInternalsInvalidationReason::ZoomChanged,
        )),
        NodeMeasurementOutcome::Changed
    );

    assert_eq!(
        store.node_measurement_status(a),
        NodeMeasurementStatus::Dirty {
            revision: 1,
            reason: NodeInternalsInvalidationReason::ZoomChanged,
        }
    );
    assert_eq!(
        store.node_measurement_status(b),
        NodeMeasurementStatus::Dirty {
            revision: 2,
            reason: NodeInternalsInvalidationReason::ZoomChanged,
        }
    );
    assert_eq!(
        store.node_measurement_status(c),
        NodeMeasurementStatus::Fresh { revision: 3 }
    );
}

#[test]
fn semantic_measurement_facts_serialize_without_renderer_types() {
    let node = NodeId::from_u128(90);
    let port = PortId::from_u128(91);
    let measurement = NodeMeasurement::new(node)
        .with_revision(2)
        .with_size(Some(CanvasSize {
            width: 160.0,
            height: 120.0,
        }))
        .with_slots([MeasuredSurfaceSlot::new(
            "field.prompt",
            CanvasRect {
                origin: CanvasPoint { x: 8.0, y: 40.0 },
                size: CanvasSize {
                    width: 144.0,
                    height: 28.0,
                },
            },
        )
        .with_visibility(NodeSurfaceSlotVisibility::Visible)])
        .with_anchors([MeasuredSurfaceAnchor::new(
            "field.prompt.input",
            CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 44.0 },
                size: CanvasSize {
                    width: 10.0,
                    height: 20.0,
                },
            },
            HandlePosition::Left,
        )
        .with_port(port)
        .with_port_key("p")]);

    let json = serde_json::to_string(&measurement).expect("serialize measurement");
    assert!(json.contains("field.prompt"));
    assert!(!json.contains("egui"));
    assert!(!json.contains("gpui"));
    assert!(!json.contains("dioxus"));

    let roundtrip: NodeMeasurement = serde_json::from_str(&json).expect("deserialize measurement");
    assert_eq!(roundtrip, measurement);
}

#[test]
fn clearing_measurement_removes_derived_handle_facts() {
    let source = NodeId::from_u128(40);
    let target = NodeId::from_u128(41);
    let out = PortId::from_u128(42);
    let input = PortId::from_u128(43);
    let edge = EdgeId::from_u128(44);
    let mut store = NodeGraphStore::new(
        graph_with_unsized_connected_nodes(source, target, out, input, edge),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let source_handle = ConnectionHandleRef::new(source, out, PortDirection::Out);
    let target_handle = ConnectionHandleRef::new(target, input, PortDirection::In);

    store
        .report_node_measurement(
            NodeMeasurement::new(source)
                .with_size(Some(CanvasSize {
                    width: 100.0,
                    height: 100.0,
                }))
                .with_handles([MeasuredHandle::new(
                    source_handle,
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 90.0, y: 40.0 },
                            size: CanvasSize {
                                width: 10.0,
                                height: 20.0,
                            },
                        },
                        position: HandlePosition::Right,
                    },
                )]),
        )
        .expect("source measurement");
    store
        .report_node_measurement(
            NodeMeasurement::new(target)
                .with_size(Some(CanvasSize {
                    width: 100.0,
                    height: 100.0,
                }))
                .with_handles([MeasuredHandle::new(
                    target_handle,
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 0.0, y: 40.0 },
                            size: CanvasSize {
                                width: 10.0,
                                height: 20.0,
                            },
                        },
                        position: HandlePosition::Left,
                    },
                )]),
        )
        .expect("target measurement");

    assert!(store.edge_position_from_layout_facts(edge).is_some());
    assert!(
        store
            .resolve_connection_target_from_layout_facts(
                CanvasPoint { x: 205.0, y: 50.0 },
                source_handle,
            )
            .target
            .is_some()
    );

    assert_eq!(
        store.clear_node_measurement(target),
        NodeMeasurementOutcome::Changed
    );
    assert!(store.edge_position_from_layout_facts(edge).is_none());
    let facts = store.layout_facts_query(CanvasSize {
        width: 320.0,
        height: 160.0,
    });
    assert!(facts.visible_edge_position(edge).is_none());
    assert!(
        facts
            .connection_target_candidates
            .iter()
            .all(|candidate| candidate.target.handle != target_handle)
    );
    let resolved = store.resolve_connection_target_from_layout_facts(
        CanvasPoint { x: 205.0, y: 50.0 },
        source_handle,
    );
    assert!(resolved.target.is_none());
    assert!(!resolved.is_handle_valid);
}

fn report_basic_connected_measurements(
    store: &mut NodeGraphStore,
    source: NodeId,
    target: NodeId,
    out: PortId,
    input: PortId,
) {
    let source_handle = ConnectionHandleRef::new(source, out, PortDirection::Out);
    let target_handle = ConnectionHandleRef::new(target, input, PortDirection::In);
    store
        .report_node_measurement(
            NodeMeasurement::new(source)
                .with_size(Some(CanvasSize {
                    width: 100.0,
                    height: 100.0,
                }))
                .with_handles([MeasuredHandle::new(
                    source_handle,
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 90.0, y: 40.0 },
                            size: CanvasSize {
                                width: 10.0,
                                height: 20.0,
                            },
                        },
                        position: HandlePosition::Right,
                    },
                )]),
        )
        .expect("source measurement");
    store
        .report_node_measurement(
            NodeMeasurement::new(target)
                .with_size(Some(CanvasSize {
                    width: 100.0,
                    height: 100.0,
                }))
                .with_handles([MeasuredHandle::new(
                    target_handle,
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 0.0, y: 40.0 },
                            size: CanvasSize {
                                width: 10.0,
                                height: 20.0,
                            },
                        },
                        position: HandlePosition::Left,
                    },
                )]),
        )
        .expect("target measurement");
}

#[test]
fn invalid_measurement_input_is_rejected_without_replacing_existing_facts() {
    let node = NodeId::from_u128(50);
    let mut store = NodeGraphStore::new(
        graph_with_unsized_node(node),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let measurement = NodeMeasurement::new(node).with_size(Some(CanvasSize {
        width: 10.0,
        height: 10.0,
    }));

    store
        .report_node_measurement(measurement.clone())
        .expect("valid measurement");
    let before = store.node_measurement(node);

    let err = store
        .report_node_measurement(NodeMeasurement::new(node).with_size(Some(CanvasSize {
            width: 0.0,
            height: 10.0,
        })))
        .expect_err("invalid size should be rejected");

    assert!(matches!(err, NodeMeasurementError::InvalidSize { .. }));
    assert_eq!(store.node_measurement(node), before);
}

fn graph_with_unsized_node(node: NodeId) -> Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(
        node,
        Node {
            pos: CanvasPoint::default(),
            ..node_fixture(Vec::new())
        },
    );
    graph.into()
}

fn graph_with_unsized_connected_nodes(
    source: NodeId,
    target: NodeId,
    out: PortId,
    input: PortId,
    edge: EdgeId,
) -> Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(2));
    graph.insert_node(
        source,
        Node {
            pos: CanvasPoint::default(),
            ports: vec![out],
            ..node_fixture(Vec::new())
        },
    );
    graph.insert_node(
        target,
        Node {
            pos: CanvasPoint { x: 200.0, y: 0.0 },
            ports: vec![input],
            ..node_fixture(Vec::new())
        },
    );
    graph.insert_port(out, port_fixture(source, PortDirection::Out));
    graph.insert_port(input, port_fixture(target, PortDirection::In));
    graph.insert_edge(edge, Edge::new(EdgeKind::Data, out, input));
    graph.into()
}

fn node_fixture(ports: Vec<PortId>) -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos: CanvasPoint::default(),
        origin: None,
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden: false,
        collapsed: false,
        ports,
        data: serde_json::Value::Null,
    }
}

fn port_fixture(node: NodeId, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new("p"),
        dir: direction,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}
