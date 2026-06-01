use std::cell::RefCell;
use std::rc::Rc;

use super::fixtures::{make_graph, make_store};

use crate::runtime::events::{NodeGraphStoreEvent, ViewChange};
use crate::runtime::geometry::{
    BezierEdgeOptions, EdgeEndpointInput, EdgeHitTestOptions, HandleBounds, HandlePosition,
    bezier_edge_path, edge_path_contains_point, edge_position,
};
use crate::runtime::xyflow::callbacks::{
    ConnectionChange, connection_changes_from_transaction, delete_changes_from_transaction,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, NodeId, Port, PortCapacity,
    PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::ops::{EdgeEndpoints, GraphOp, GraphTransaction};

#[test]
fn adapter_conformance_connect_dispatches_patch_and_xyflow_projection() {
    let (graph, _a, _b, out_port, in_port, _eid) = make_graph();
    let mut store = make_store(graph);
    let edge_id = EdgeId::new();
    let edge = Edge {
        kind: EdgeKind::Data,
        from: out_port,
        to: in_port,
        selectable: None,
        deletable: None,
        reconnectable: None,
    };

    let tx = GraphTransaction::from_ops([GraphOp::AddEdge { id: edge_id, edge }])
        .with_label("adapter connect");
    let outcome = store.dispatch_transaction(&tx).expect("dispatch connect");
    let changes = NodeGraphChanges::from_patch(&outcome.patch);
    let connection_changes = connection_changes_from_transaction(outcome.committed());

    assert_eq!(outcome.committed().label(), Some("adapter connect"));
    assert!(store.graph().edges.contains_key(&edge_id));
    assert!(
        matches!(changes.edges(), [EdgeChange::Add { id, .. }] if *id == edge_id),
        "connect should project to one edge add",
    );
    assert!(
        matches!(connection_changes.as_slice(), [ConnectionChange::Connected(conn)]
            if conn.edge == edge_id && conn.from == out_port && conn.to == in_port),
        "connect should project to one connection event",
    );
}

#[test]
fn adapter_conformance_reconnect_preserves_edge_id_and_projects_endpoint_change() {
    let (mut graph, _a, b, out_port, in_port, edge_id) = make_graph();
    let next_in = insert_input_port(&mut graph, b, "in2");
    let mut store = make_store(graph);
    let from = EdgeEndpoints {
        from: out_port,
        to: in_port,
    };
    let to = EdgeEndpoints {
        from: out_port,
        to: next_in,
    };

    let tx = GraphTransaction::from_ops([GraphOp::SetEdgeEndpoints {
        id: edge_id,
        from,
        to,
    }])
    .with_label("adapter reconnect");
    let outcome = store.dispatch_transaction(&tx).expect("dispatch reconnect");
    let changes = NodeGraphChanges::from_patch(&outcome.patch);
    let connection_changes = connection_changes_from_transaction(outcome.committed());

    let edge = store.graph().edges.get(&edge_id).expect("edge remains");
    assert_eq!(edge.from, out_port);
    assert_eq!(edge.to, next_in);
    assert!(
        matches!(changes.edges(), [EdgeChange::Endpoints { id, from, to }]
            if *id == edge_id && *from == out_port && *to == next_in),
        "reconnect should project to one endpoint change",
    );
    assert!(
        matches!(connection_changes.as_slice(), [ConnectionChange::Reconnected { edge, from: old, to: new }]
            if *edge == edge_id && *old == from && *new == to),
        "reconnect should preserve edge id and expose old/new endpoints",
    );
}

#[test]
fn adapter_conformance_delete_node_cascades_edges_and_projects_delete_payload() {
    let (graph, node_id, _b, out_port, _in_port, edge_id) = make_graph();
    let node = graph.nodes.get(&node_id).expect("node").clone();
    let port = graph.ports.get(&out_port).expect("port").clone();
    let edge = graph.edges.get(&edge_id).expect("edge").clone();
    let mut store = make_store(graph);

    let tx = GraphTransaction::from_ops([GraphOp::RemoveNode {
        id: node_id,
        node,
        ports: vec![(out_port, port)],
        edges: vec![(edge_id, edge)],
    }])
    .with_label("adapter delete node");
    let outcome = store
        .dispatch_transaction(&tx)
        .expect("dispatch delete node");
    let changes = NodeGraphChanges::from_patch(&outcome.patch);
    let deleted = delete_changes_from_transaction(outcome.committed());

    assert!(!store.graph().nodes.contains_key(&node_id));
    assert!(!store.graph().edges.contains_key(&edge_id));
    assert!(
        matches!(changes.nodes(), [NodeChange::Remove { id }] if *id == node_id),
        "delete should project to one node remove",
    );
    assert!(
        matches!(changes.edges(), [EdgeChange::Remove { id }] if *id == edge_id),
        "delete should project cascaded edge removal",
    );
    assert_eq!(deleted.nodes(), &[node_id]);
    assert_eq!(deleted.edges(), &[edge_id]);
}

#[test]
fn adapter_conformance_viewport_and_selection_emit_ordered_view_changes() {
    let (graph, node_id, _b, _out_port, _in_port, edge_id) = make_graph();
    let mut store = make_store(graph);
    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();

    store.subscribe(move |event| {
        if let NodeGraphStoreEvent::ViewChanged { changes, .. } = event {
            match changes {
                [ViewChange::Viewport { pan, zoom }] => {
                    assert_eq!(*pan, CanvasPoint { x: 10.0, y: 20.0 });
                    assert_eq!(*zoom, 1.25);
                    events2.borrow_mut().push("viewport");
                }
                [
                    ViewChange::Selection {
                        nodes,
                        edges,
                        groups,
                    },
                ] => {
                    assert_eq!(nodes, &[node_id]);
                    assert_eq!(edges, &[edge_id]);
                    assert!(groups.is_empty());
                    events2.borrow_mut().push("selection");
                }
                _ => events2.borrow_mut().push("unexpected"),
            }
        }
    });

    store.set_viewport(CanvasPoint { x: 10.0, y: 20.0 }, 1.25);
    store.set_selection(vec![node_id], vec![edge_id], Vec::new());

    assert_eq!(events.borrow().as_slice(), &["viewport", "selection"]);
}

#[test]
fn adapter_conformance_geometry_hit_test_is_renderer_neutral() {
    let source_node = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 120.0,
            height: 80.0,
        },
    };
    let target_node = CanvasRect {
        origin: CanvasPoint { x: 240.0, y: 40.0 },
        size: CanvasSize {
            width: 120.0,
            height: 80.0,
        },
    };

    let endpoints = edge_position(
        EdgeEndpointInput {
            node_rect: source_node,
            handle: Some(HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 112.0, y: 32.0 },
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
            node_rect: target_node,
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
    let path = bezier_edge_path(
        endpoints.source,
        endpoints.target,
        BezierEdgeOptions::default(),
    )
    .expect("bezier path");

    assert!(edge_path_contains_point(
        &path,
        path.label.point,
        EdgeHitTestOptions::default(),
    ));
}

fn insert_input_port(graph: &mut jellyflow_core::core::Graph, node: NodeId, key: &str) -> PortId {
    let port_id = PortId::new();
    graph
        .nodes
        .get_mut(&node)
        .expect("node exists")
        .ports
        .push(port_id);
    graph.ports.insert(
        port_id,
        Port {
            node,
            key: PortKey::new(key),
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
    port_id
}
