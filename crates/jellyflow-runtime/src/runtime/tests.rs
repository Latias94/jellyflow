use crate::io::NodeGraphViewState;
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::events::NodeGraphStoreEvent;
use crate::runtime::lookups::{ConnectionSide, NodeGraphLookups};
use crate::runtime::middleware::NodeGraphStoreMiddleware;
use crate::runtime::store::NodeGraphStore;
use crate::runtime::xyflow::apply::{apply_edge_changes, apply_node_changes};
use crate::runtime::xyflow::callbacks::{
    ConnectionChange, NodeGraphCommitCallbacks, NodeGraphGestureCallbacks, NodeGraphViewCallbacks,
    connection_changes_from_transaction, install_callbacks,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, EdgeReconnectable, Graph, GraphId,
    Group, GroupId, Node, NodeExtent, NodeId, NodeKindKey, Port, PortKind,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

fn default_editor_config() -> crate::io::NodeGraphEditorConfig {
    crate::io::NodeGraphEditorConfig::default()
}

fn make_graph() -> (
    Graph,
    NodeId,
    NodeId,
    jellyflow_core::core::PortId,
    jellyflow_core::core::PortId,
    EdgeId,
) {
    let mut g = Graph::new(jellyflow_core::core::GraphId::from_u128(1));

    let a = NodeId::new();
    let b = NodeId::new();

    let out_port = jellyflow_core::core::PortId::new();
    let in_port = jellyflow_core::core::PortId::new();

    let node_a = Node {
        kind: NodeKindKey::new("demo.a"),
        kind_version: 1,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden: false,
        collapsed: false,
        ports: vec![out_port],
        data: serde_json::Value::Null,
    };
    let node_b = Node {
        kind: NodeKindKey::new("demo.b"),
        kind_version: 1,
        pos: CanvasPoint { x: 100.0, y: 0.0 },
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden: false,
        collapsed: false,
        ports: vec![in_port],
        data: serde_json::Value::Null,
    };

    g.nodes.insert(a, node_a);
    g.nodes.insert(b, node_b);
    g.ports.insert(
        out_port,
        Port {
            node: a,
            key: jellyflow_core::core::PortKey::new("out"),
            dir: jellyflow_core::core::PortDirection::Out,
            kind: jellyflow_core::core::PortKind::Data,
            capacity: jellyflow_core::core::PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );
    g.ports.insert(
        in_port,
        Port {
            node: b,
            key: jellyflow_core::core::PortKey::new("in"),
            dir: jellyflow_core::core::PortDirection::In,
            kind: jellyflow_core::core::PortKind::Data,
            capacity: jellyflow_core::core::PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );

    let eid = EdgeId::new();
    g.edges.insert(
        eid,
        Edge {
            kind: EdgeKind::Data,
            from: out_port,
            to: in_port,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    (g, a, b, out_port, in_port, eid)
}

mod store;
mod xyflow;

#[test]
fn lookups_rebuild_populates_connection_lookup() {
    let (g, a, b, out_port, in_port, eid) = make_graph();

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    assert!(lookups.node_lookup.contains_key(&a));
    assert!(lookups.node_lookup.contains_key(&b));
    assert_eq!(lookups.node_lookup.get(&a).unwrap().ports, vec![out_port]);
    assert_eq!(lookups.node_lookup.get(&b).unwrap().ports, vec![in_port]);

    assert_eq!(lookups.edge_lookup.get(&eid).unwrap().from, out_port);
    assert_eq!(lookups.edge_lookup.get(&eid).unwrap().to, in_port);

    let a_out = lookups
        .connections_for_port(a, ConnectionSide::Source, out_port)
        .expect("connections");
    assert_eq!(a_out.get(&eid).unwrap().target_node, b);

    let b_all = lookups.connections_for_node(b).expect("connections");
    assert!(b_all.contains_key(&eid));
}

#[test]
fn lookups_connections_for_node_side_filters_by_direction() {
    let (g, a, b, out_port, in_port, eid) = make_graph();

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let a_source = lookups
        .connections_for_node_side(a, ConnectionSide::Source)
        .expect("connections");
    assert!(a_source.contains_key(&eid));

    let a_target = lookups.connections_for_node_side(a, ConnectionSide::Target);
    assert!(a_target.is_none() || !a_target.unwrap().contains_key(&eid));

    let b_target = lookups
        .connections_for_node_side(b, ConnectionSide::Target)
        .expect("connections");
    assert!(b_target.contains_key(&eid));

    let b_source = lookups.connections_for_node_side(b, ConnectionSide::Source);
    assert!(b_source.is_none() || !b_source.unwrap().contains_key(&eid));

    let _ = (out_port, in_port);
}
