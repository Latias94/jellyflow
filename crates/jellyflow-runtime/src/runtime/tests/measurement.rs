use crate::io::{NodeGraphEditorConfig, NodeGraphViewState};
use crate::runtime::connection::{ConnectionHandleRef, ConnectionHandleValidity};
use crate::runtime::geometry::{HandleBounds, HandlePosition};
use crate::runtime::measurement::{MeasuredHandle, NodeMeasurement, NodeMeasurementOutcome};
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

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
        store.graph().nodes.get(&node).expect("node exists").size,
        None,
        "runtime measurements must not persist into Graph"
    );

    assert_eq!(
        store.clear_node_measurement(node),
        NodeMeasurementOutcome::Changed
    );
    assert!(store.rendering_query(viewport).visible_node_ids.is_empty());
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
        .edge_position_from_measurements(edge)
        .expect("edge endpoints");
    assert_eq!(endpoints.source.point, CanvasPoint { x: 100.0, y: 50.0 });
    assert_eq!(endpoints.target.point, CanvasPoint { x: 200.0, y: 50.0 });

    let target = store.resolve_connection_target_from_measurements(
        CanvasPoint { x: 205.0, y: 50.0 },
        source_handle,
    );
    assert_eq!(target.target.expect("target handle").handle, target_handle);
    assert_eq!(target.feedback, ConnectionHandleValidity::Valid);
    assert!(target.is_handle_valid);
}

fn graph_with_unsized_node(node: NodeId) -> Graph {
    let mut graph = Graph::new(GraphId::from_u128(1));
    graph.nodes.insert(
        node,
        Node {
            pos: CanvasPoint::default(),
            ..node_fixture(Vec::new())
        },
    );
    graph
}

fn graph_with_unsized_connected_nodes(
    source: NodeId,
    target: NodeId,
    out: PortId,
    input: PortId,
    edge: EdgeId,
) -> Graph {
    let mut graph = Graph::new(GraphId::from_u128(2));
    graph.nodes.insert(
        source,
        Node {
            pos: CanvasPoint::default(),
            ports: vec![out],
            ..node_fixture(Vec::new())
        },
    );
    graph.nodes.insert(
        target,
        Node {
            pos: CanvasPoint { x: 200.0, y: 0.0 },
            ports: vec![input],
            ..node_fixture(Vec::new())
        },
    );
    graph
        .ports
        .insert(out, port_fixture(source, PortDirection::Out));
    graph
        .ports
        .insert(input, port_fixture(target, PortDirection::In));
    graph.edges.insert(
        edge,
        Edge {
            kind: EdgeKind::Data,
            from: out,
            to: input,
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
