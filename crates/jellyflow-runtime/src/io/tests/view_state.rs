use super::*;
use jellyflow_core::core::{
    Edge, EdgeId, EdgeKind, Graph, GraphId, NodeId, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};

#[test]
fn view_state_sanitize_removes_stale_ids() {
    let graph_id = GraphId::new();
    let mut graph = Graph::new(graph_id);

    let keep_node = NodeId::new();
    graph.nodes.insert(
        keep_node,
        jellyflow_core::core::Node {
            kind: jellyflow_core::core::NodeKindKey::new("test"),
            kind_version: 1,
            pos: jellyflow_core::core::CanvasPoint { x: 0.0, y: 0.0 },
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
            ports: Vec::new(),
            data: serde_json::Value::Null,
        },
    );
    let from_port = PortId::new();
    let to_port = PortId::new();
    let hidden_edge = EdgeId::new();
    graph.ports.insert(
        from_port,
        Port {
            node: keep_node,
            key: PortKey::new("from"),
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
        to_port,
        Port {
            node: keep_node,
            key: PortKey::new("to"),
            dir: PortDirection::In,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );
    graph.edges.insert(
        hidden_edge,
        Edge {
            kind: EdgeKind::Data,
            from: from_port,
            to: to_port,
            hidden: true,
            selectable: None,
            focusable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let mut state = NodeGraphViewState {
        selected_nodes: vec![keep_node, NodeId::new()],
        selected_edges: vec![hidden_edge],
        draw_order: vec![NodeId::new(), keep_node],
        ..NodeGraphViewState::default()
    };

    state.sanitize_for_graph(&graph);
    assert_eq!(state.selected_nodes, vec![keep_node]);
    assert!(state.selected_edges.is_empty());
    assert_eq!(state.draw_order, vec![keep_node]);
}

#[test]
fn view_state_sanitize_normalizes_invalid_viewport() {
    let graph = Graph::new(GraphId::new());
    let mut state = NodeGraphViewState {
        pan: jellyflow_core::core::CanvasPoint {
            x: f32::INFINITY,
            y: 10.0,
        },
        zoom: -1.0,
        ..NodeGraphViewState::default()
    };

    state.sanitize_for_graph(&graph);

    assert_eq!(state.pan, jellyflow_core::core::CanvasPoint::default());
    assert_eq!(state.zoom, NodeGraphViewState::default().zoom);

    state.set_viewport(
        jellyflow_core::core::CanvasPoint {
            x: 1.0,
            y: f32::NAN,
        },
        f32::NAN,
    );

    assert_eq!(state.pan, jellyflow_core::core::CanvasPoint::default());
    assert_eq!(state.zoom, NodeGraphViewState::default().zoom);
}
