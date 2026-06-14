use crate::runtime::binding::BindingQueryOptions;
use crate::runtime::measurement::{MeasuredHandle, NodeMeasurement};
use crate::runtime::query;
use crate::runtime::tests::fixtures::{
    GraphFixtureUpdateExt, fixture_insert_binding, make_graph, make_store,
};
use crate::runtime::{
    connection::ConnectionHandleRef,
    geometry::{HandleBounds, HandlePosition},
};
use crate::{
    io::{NodeGraphEditorConfig, NodeGraphNodeOrigin, NodeGraphRuntimeTuning, NodeGraphViewState},
    runtime::store::NodeGraphStore,
};
use jellyflow_core::core::{
    Binding, BindingEndpoint, BindingId, CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId,
    EdgeKind, Graph, GraphBuilder, GraphId, GraphLocalBindingTarget, Node, NodeId, NodeKindKey,
    Port, PortCapacity, PortDirection, PortId, PortKey, PortKind, SourceAnchor,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn linear_query_backend_matches_store_facades() {
    let (mut graph, source, _target, out, input, _edge) = make_graph();
    graph
        .update_node(&source, |node| {
            node.size = Some(CanvasSize {
                width: 100.0,
                height: 80.0,
            })
        })
        .expect("node exists");
    let binding = BindingId::from_u128(10);
    fixture_insert_binding(&mut graph, binding, source_binding(source));
    let mut store = make_store(graph);
    let source_handle = ConnectionHandleRef::new(source, out, jellyflow_core::PortDirection::Out);
    let target_handle = ConnectionHandleRef::new(
        store.graph().ports()[&input].node,
        input,
        jellyflow_core::PortDirection::In,
    );

    store
        .report_node_measurement(
            NodeMeasurement::new(target_handle.node)
                .with_size(Some(CanvasSize {
                    width: 120.0,
                    height: 80.0,
                }))
                .with_handles([MeasuredHandle::new(
                    target_handle,
                    HandleBounds {
                        rect: CanvasRect {
                            origin: CanvasPoint { x: 0.0, y: 30.0 },
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
    store
        .dispatch_transaction(&GraphTransaction::from_ops([GraphOp::SetNodePos {
            id: source,
            from: CanvasPoint::default(),
            to: CanvasPoint { x: 20.0, y: 20.0 },
        }]))
        .expect("move source");

    let viewport = CanvasSize {
        width: 400.0,
        height: 300.0,
    };

    assert_eq!(
        query::rendering_query(&store, viewport),
        store.rendering_query(viewport)
    );
    assert_eq!(
        query::layout_facts_query(&store, viewport),
        store.layout_facts_query(viewport)
    );
    assert_eq!(
        query::binding_query(&store, BindingQueryOptions::default()),
        store.binding_query()
    );
    assert!(
        query::connection_target_candidates_from_layout_facts(&store)
            .iter()
            .any(|candidate| candidate.target.handle == target_handle)
    );
    assert!(
        query::edge_position_from_layout_facts(
            &store,
            *store.graph().edges().keys().next().unwrap()
        )
        .is_some()
    );
    assert!(
        query::resolve_connection_target_from_layout_facts(
            &store,
            CanvasPoint { x: 105.0, y: 40.0 },
            source_handle,
        )
        .target
        .is_some()
    );
}

#[test]
fn spatial_query_backend_is_opt_in_and_matches_linear_rendering_contract() {
    assert!(!NodeGraphRuntimeTuning::default().spatial_index.enabled);

    let (graph, measured, selected, spanning, outside, _hidden_edge, hidden_endpoint, inside_edge) =
        graph_for_spatial_rendering_query();
    let view_state = NodeGraphViewState {
        selected_nodes: vec![selected],
        draw_order: vec![selected, measured],
        edge_draw_order: vec![outside, spanning, hidden_endpoint, inside_edge],
        ..NodeGraphViewState::default()
    };
    let mut linear = NodeGraphStore::new(
        graph.clone(),
        view_state.clone(),
        NodeGraphEditorConfig::default(),
    );
    let mut spatial = NodeGraphStore::new(graph, view_state, spatial_editor_config());

    report_measured_size(&mut linear, measured);
    report_measured_size(&mut spatial, measured);

    let viewport = CanvasSize {
        width: 100.0,
        height: 100.0,
    };
    let linear_result = linear.rendering_query(viewport);
    let spatial_result = spatial.rendering_query(viewport);

    assert_eq!(spatial_result, linear_result);
    assert_eq!(
        spatial_result.visible_edge_ids,
        vec![spanning, inside_edge],
        "spatial backend must keep endpoint-union edge visibility, including spanning edges whose endpoint nodes are outside the viewport"
    );
    assert!(
        !spatial_result.visible_edge_ids.contains(&hidden_endpoint),
        "hidden endpoint nodes must keep incident edges out of visibility results"
    );
}

#[test]
fn disabled_spatial_tuning_keeps_linear_backend_public_results() {
    let (graph, measured, _selected, _spanning, _outside, _hidden_edge, _hidden_endpoint, _inside) =
        graph_for_spatial_rendering_query();
    let mut default_store = NodeGraphStore::new(
        graph.clone(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let mut disabled_store = NodeGraphStore::new(graph, NodeGraphViewState::default(), {
        let mut config = NodeGraphEditorConfig::default();
        config.runtime_tuning.spatial_index.enabled = false;
        config.runtime_tuning.spatial_index.cell_size_screen_px = 8.0;
        config
    });
    report_measured_size(&mut default_store, measured);
    report_measured_size(&mut disabled_store, measured);

    let viewport = CanvasSize {
        width: 100.0,
        height: 100.0,
    };

    assert_eq!(
        disabled_store.rendering_query(viewport),
        default_store.rendering_query(viewport)
    );
    assert_eq!(
        disabled_store.layout_facts_query(viewport),
        default_store.layout_facts_query(viewport)
    );
    assert_eq!(
        disabled_store.binding_query(),
        default_store.binding_query()
    );
}

#[test]
fn spatial_rendering_query_matches_linear_with_culling_disabled() {
    let (graph, measured, _selected, spanning, outside, hidden_edge, hidden_endpoint, inside_edge) =
        graph_for_spatial_rendering_query();
    let mut linear_config = NodeGraphEditorConfig::default();
    linear_config.runtime_tuning.only_render_visible_elements = false;
    let mut spatial_config = spatial_editor_config();
    spatial_config.runtime_tuning.only_render_visible_elements = false;
    let mut linear =
        NodeGraphStore::new(graph.clone(), NodeGraphViewState::default(), linear_config);
    let mut spatial = NodeGraphStore::new(graph, NodeGraphViewState::default(), spatial_config);
    report_measured_size(&mut linear, measured);
    report_measured_size(&mut spatial, measured);

    let viewport = CanvasSize {
        width: 100.0,
        height: 100.0,
    };
    let spatial_result = spatial.rendering_query(viewport);

    assert_eq!(spatial_result, linear.rendering_query(viewport));
    assert_eq!(
        spatial_result.visible_edge_ids,
        vec![spanning, outside, hidden_endpoint, inside_edge],
        "disabled culling returns all non-hidden edges, matching linear behavior"
    );
    assert!(!spatial_result.visible_edge_ids.contains(&hidden_edge));
}

#[test]
fn spatial_rendering_query_matches_linear_for_invalid_viewport_with_culling_disabled() {
    let (graph, measured, _selected, _spanning, _outside, _hidden_edge, _hidden_endpoint, _inside) =
        graph_for_spatial_rendering_query();
    let linear_config = NodeGraphEditorConfig::default().with_only_render_visible_elements(false);
    let spatial_config = spatial_editor_config().with_only_render_visible_elements(false);
    let mut linear =
        NodeGraphStore::new(graph.clone(), NodeGraphViewState::default(), linear_config);
    let mut spatial = NodeGraphStore::new(graph, NodeGraphViewState::default(), spatial_config);
    report_measured_size(&mut linear, measured);
    report_measured_size(&mut spatial, measured);

    let viewport = CanvasSize {
        width: 0.0,
        height: 100.0,
    };

    assert_eq!(
        spatial.rendering_query(viewport),
        linear.rendering_query(viewport)
    );
    assert!(
        spatial
            .rendering_query(viewport)
            .visible_node_ids
            .is_empty()
    );
    assert!(
        spatial
            .rendering_query(viewport)
            .visible_edge_ids
            .is_empty()
    );
}

#[test]
fn spatial_rendering_query_matches_linear_for_node_origin_override() {
    let node = NodeId::from_u128(900);
    let mut graph = GraphBuilder::new(GraphId::from_u128(900));
    graph.insert_node(
        node,
        Node {
            pos: CanvasPoint { x: 15.0, y: 0.0 },
            size: Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
            ..node_model("test.centered", false)
        },
    );
    let mut linear_config = NodeGraphEditorConfig::default();
    linear_config.interaction.node_origin = NodeGraphNodeOrigin { x: 0.5, y: 0.0 };
    let mut spatial_config = linear_config.clone();
    spatial_config.runtime_tuning.spatial_index.enabled = true;
    spatial_config
        .runtime_tuning
        .spatial_index
        .cell_size_screen_px = 8.0;
    let view_state = NodeGraphViewState {
        pan: CanvasPoint { x: -9.5, y: 0.0 },
        zoom: 1.0,
        ..NodeGraphViewState::default()
    };
    let graph: Graph = graph.into();
    let linear = NodeGraphStore::new(graph.clone(), view_state.clone(), linear_config);
    let spatial = NodeGraphStore::new(graph, view_state, spatial_config);
    let viewport = CanvasSize {
        width: 1.0,
        height: 10.0,
    };

    assert_eq!(
        spatial.rendering_query(viewport),
        linear.rendering_query(viewport)
    );
    assert_eq!(
        spatial.rendering_query(viewport).visible_node_ids,
        vec![node]
    );
}

#[test]
fn spatial_rendering_query_matches_linear_for_full_viewport() {
    let (graph, measured, _selected, spanning, outside, hidden_edge, hidden_endpoint, inside_edge) =
        graph_for_spatial_rendering_query();
    let mut linear = NodeGraphStore::new(
        graph.clone(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let mut spatial = NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        spatial_editor_config(),
    );
    report_measured_size(&mut linear, measured);
    report_measured_size(&mut spatial, measured);

    let viewport = CanvasSize {
        width: 200.0,
        height: 120.0,
    };
    let spatial_result = spatial.rendering_query(viewport);

    assert_eq!(spatial_result, linear.rendering_query(viewport));
    assert_eq!(
        spatial_result.visible_edge_ids,
        vec![spanning, outside, inside_edge],
        "full-view spatial fast path keeps endpoint and hidden-edge visibility semantics"
    );
    assert!(!spatial_result.visible_edge_ids.contains(&hidden_edge));
    assert!(!spatial_result.visible_edge_ids.contains(&hidden_endpoint));
}

#[test]
fn spatial_rendering_query_reuses_cached_node_index_for_repeated_and_panned_reads() {
    let (graph, measured, _selected, _spanning, _outside, _hidden_edge, _hidden_endpoint, _inside) =
        graph_for_spatial_rendering_query();
    let mut store = NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        spatial_editor_config(),
    );
    report_measured_size(&mut store, measured);
    let viewport = CanvasSize {
        width: 100.0,
        height: 100.0,
    };

    assert_eq!(spatial_node_index_build_count(&store), 0);
    let first = store.rendering_query(viewport);
    assert_eq!(
        spatial_node_index_build_count(&store),
        1,
        "one rendering query should build one shared spatial node index for nodes and edges"
    );
    assert_eq!(store.rendering_query(viewport), first);
    assert_eq!(
        spatial_node_index_build_count(&store),
        1,
        "repeated reads over the same graph/layout/config should reuse the cached index"
    );

    store.set_viewport(CanvasPoint { x: -24.0, y: -16.0 }, 1.0);
    let _ = store.rendering_query(viewport);
    assert_eq!(
        spatial_node_index_build_count(&store),
        1,
        "pan-only viewport changes query different cells without rebuilding the node index"
    );
}

#[test]
fn spatial_rendering_query_rebuilds_cached_node_index_when_geometry_inputs_change() {
    let (graph, measured, _selected, _spanning, _outside, _hidden_edge, _hidden_endpoint, _inside) =
        graph_for_spatial_rendering_query();
    let mut store = NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        spatial_editor_config(),
    );
    let viewport = CanvasSize {
        width: 100.0,
        height: 100.0,
    };

    let _ = store.rendering_query(viewport);
    assert_eq!(spatial_node_index_build_count(&store), 1);

    report_measured_size(&mut store, measured);
    let _ = store.rendering_query(viewport);
    assert_eq!(
        spatial_node_index_build_count(&store),
        2,
        "measurement changes affect node bounds and must invalidate the cached index"
    );

    store
        .dispatch_transaction(&GraphTransaction::from_ops([GraphOp::SetNodePos {
            id: measured,
            from: CanvasPoint { x: 30.0, y: 0.0 },
            to: CanvasPoint { x: 32.0, y: 0.0 },
        }]))
        .expect("move measured node");
    let _ = store.rendering_query(viewport);
    assert_eq!(
        spatial_node_index_build_count(&store),
        3,
        "graph edits must invalidate the cached index"
    );

    store.set_viewport(CanvasPoint::default(), 2.0);
    let _ = store.rendering_query(viewport);
    assert_eq!(
        spatial_node_index_build_count(&store),
        4,
        "zoom changes that cross a spatial cell-size bucket must rebuild the cached index"
    );

    store.update_editor_config(|config| {
        config.interaction.node_origin = NodeGraphNodeOrigin { x: 0.5, y: 0.0 };
    });
    let _ = store.rendering_query(viewport);
    assert_eq!(
        spatial_node_index_build_count(&store),
        5,
        "node-origin changes alter node bounds and must rebuild the cached index"
    );
}

#[test]
fn spatial_rendering_query_reuses_cached_node_index_for_small_zoom_changes() {
    let (graph, measured, _selected, _spanning, _outside, _hidden_edge, _hidden_endpoint, _inside) =
        graph_for_spatial_rendering_query();
    let mut store = NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        spatial_editor_config(),
    );
    report_measured_size(&mut store, measured);
    let viewport = CanvasSize {
        width: 100.0,
        height: 100.0,
    };

    let _ = store.rendering_query(viewport);
    assert_eq!(spatial_node_index_build_count(&store), 1);

    store.set_viewport(CanvasPoint::default(), 1.01);
    let _ = store.rendering_query(viewport);
    assert_eq!(
        spatial_node_index_build_count(&store),
        1,
        "small zoom changes that stay in the same spatial cell-size bucket should reuse the cached index"
    );

    store.set_viewport(CanvasPoint::default(), 2.0);
    let _ = store.rendering_query(viewport);
    assert_eq!(
        spatial_node_index_build_count(&store),
        2,
        "larger zoom changes should rebuild once they cross a spatial cell-size bucket"
    );
}

fn source_binding(node: jellyflow_core::NodeId) -> Binding {
    Binding {
        subject: BindingEndpoint::graph_local(GraphLocalBindingTarget::Node { id: node }),
        target: BindingEndpoint::source(SourceAnchor::new(
            "source.pdf",
            serde_json::json!({ "page": 1 }),
        )),
        kind: Some("excerpt".to_string()),
        meta: serde_json::Value::Null,
    }
}

fn spatial_editor_config() -> NodeGraphEditorConfig {
    let mut config = NodeGraphEditorConfig::default().with_spatial_index_enabled(true);
    config.runtime_tuning.spatial_index.cell_size_screen_px = 8.0;
    config.runtime_tuning.spatial_index.min_cell_size_screen_px = 4.0;
    config
}

fn spatial_node_index_build_count(store: &NodeGraphStore) -> u64 {
    store
        .spatial_query_cache()
        .borrow()
        .node_index_build_count()
}

type SpatialRenderingFixture = (
    Graph,
    NodeId,
    NodeId,
    EdgeId,
    EdgeId,
    EdgeId,
    EdgeId,
    EdgeId,
);

fn graph_for_spatial_rendering_query() -> SpatialRenderingFixture {
    let mut graph = GraphBuilder::new(GraphId::from_u128(700));
    let size = CanvasSize {
        width: 10.0,
        height: 10.0,
    };
    let inside = NodeId::from_u128(1);
    let partial = NodeId::from_u128(2);
    let outside_left = NodeId::from_u128(3);
    let outside_right = NodeId::from_u128(4);
    let outside_far_a = NodeId::from_u128(5);
    let outside_far_b = NodeId::from_u128(6);
    let hidden = NodeId::from_u128(7);
    let measured = NodeId::from_u128(8);
    let unmeasured = NodeId::from_u128(9);
    let selected = NodeId::from_u128(10);

    for (id, pos, hidden) in [
        (inside, CanvasPoint { x: 20.0, y: 20.0 }, false),
        (partial, CanvasPoint { x: 95.0, y: 95.0 }, false),
        (outside_left, CanvasPoint { x: -20.0, y: 10.0 }, false),
        (outside_right, CanvasPoint { x: 110.0, y: 10.0 }, false),
        (outside_far_a, CanvasPoint { x: 140.0, y: 0.0 }, false),
        (outside_far_b, CanvasPoint { x: 160.0, y: 0.0 }, false),
        (hidden, CanvasPoint { x: 0.0, y: 0.0 }, true),
        (selected, CanvasPoint { x: 40.0, y: 40.0 }, false),
    ] {
        graph.insert_node(id, sized_node_model("test.spatial", pos, size, hidden));
    }
    graph.insert_node(
        measured,
        Node {
            pos: CanvasPoint { x: 30.0, y: 0.0 },
            ..node_model("test.measured", false)
        },
    );
    graph.insert_node(
        unmeasured,
        Node {
            pos: CanvasPoint { x: 30.0, y: 20.0 },
            ..node_model("test.unmeasured", false)
        },
    );

    let left_out = PortId::from_u128(101);
    let right_in = PortId::from_u128(102);
    let far_out = PortId::from_u128(103);
    let far_in = PortId::from_u128(104);
    let inside_out = PortId::from_u128(105);
    let hidden_out = PortId::from_u128(106);
    let selected_in = PortId::from_u128(107);
    let partial_in = PortId::from_u128(108);
    for (id, port) in [
        (left_out, port_model(outside_left, PortDirection::Out)),
        (right_in, port_model(outside_right, PortDirection::In)),
        (far_out, port_model(outside_far_a, PortDirection::Out)),
        (far_in, port_model(outside_far_b, PortDirection::In)),
        (inside_out, port_model(inside, PortDirection::Out)),
        (hidden_out, port_model(hidden, PortDirection::Out)),
        (selected_in, port_model(selected, PortDirection::In)),
        (partial_in, port_model(partial, PortDirection::In)),
    ] {
        graph.insert_port(id, port);
    }
    for (node, ports) in [
        (outside_left, vec![left_out]),
        (outside_right, vec![right_in]),
        (outside_far_a, vec![far_out]),
        (outside_far_b, vec![far_in]),
        (inside, vec![inside_out]),
        (hidden, vec![hidden_out]),
        (selected, vec![selected_in]),
        (partial, vec![partial_in]),
    ] {
        graph
            .update_node(&node, |node| node.ports = ports)
            .expect("node exists");
    }

    let spanning = EdgeId::from_u128(201);
    let outside = EdgeId::from_u128(202);
    let hidden_edge = EdgeId::from_u128(203);
    let hidden_endpoint = EdgeId::from_u128(204);
    let inside_edge = EdgeId::from_u128(205);
    graph.insert_edge(spanning, edge_model(left_out, right_in, false));
    graph.insert_edge(outside, edge_model(far_out, far_in, false));
    graph.insert_edge(hidden_edge, edge_model(inside_out, selected_in, true));
    graph.insert_edge(hidden_endpoint, edge_model(hidden_out, partial_in, false));
    graph.insert_edge(inside_edge, edge_model(inside_out, selected_in, false));

    (
        graph.into(),
        measured,
        selected,
        spanning,
        outside,
        hidden_edge,
        hidden_endpoint,
        inside_edge,
    )
}

fn report_measured_size(store: &mut NodeGraphStore, node: NodeId) {
    store
        .report_node_measurement(NodeMeasurement::new(node).with_size(Some(CanvasSize {
            width: 10.0,
            height: 10.0,
        })))
        .expect("node measurement");
}

fn node_model(kind: &str, hidden: bool) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
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
        hidden,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

fn sized_node_model(kind: &str, pos: CanvasPoint, size: CanvasSize, hidden: bool) -> Node {
    Node {
        pos,
        size: Some(size),
        ..node_model(kind, hidden)
    }
}

fn port_model(node: NodeId, dir: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new("p"),
        dir,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn edge_model(from: PortId, to: PortId, hidden: bool) -> Edge {
    Edge {
        kind: EdgeKind::Data,
        from,
        to,
        hidden,
        selectable: None,
        focusable: None,
        interaction_width: None,
        deletable: None,
        reconnectable: None,
    }
}
