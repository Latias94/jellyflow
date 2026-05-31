use super::*;

#[test]
fn graph_diff_is_deterministic_and_roundtrips() {
    let mut from = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    from.nodes.insert(a, make_node("core.a"));
    from.nodes.insert(b, make_node("core.b"));

    let group_id = GroupId::new();
    from.groups.insert(
        group_id,
        Group {
            title: "G".to_string(),
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
    from.nodes.get_mut(&a).unwrap().parent = Some(group_id);

    let out = PortId::from_u128(10);
    let inn = PortId::from_u128(11);
    from.ports
        .insert(out, make_port(a, "out", PortDirection::Out));
    from.ports
        .insert(inn, make_port(b, "in", PortDirection::In));
    from.nodes.get_mut(&a).unwrap().ports.push(out);
    from.nodes.get_mut(&b).unwrap().ports.push(inn);

    let edge_id = EdgeId::from_u128(123);
    from.edges.insert(
        edge_id,
        Edge {
            kind: EdgeKind::Data,
            from: out,
            to: inn,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let imported = GraphId::from_u128(10);
    from.imports.insert(imported, GraphImport::default());

    let symbol_id = SymbolId::from_u128(1);
    from.symbols.insert(
        symbol_id,
        Symbol {
            name: "S".to_string(),
            ty: None,
            default_value: None,
            meta: serde_json::Value::Null,
        },
    );

    let note_id = StickyNoteId::new();
    from.sticky_notes.insert(
        note_id,
        StickyNote {
            text: "N".to_string(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 5.0, y: 6.0 },
                size: CanvasSize {
                    width: 7.0,
                    height: 8.0,
                },
            },
            color: None,
        },
    );

    let mut to = from.clone();
    to.imports.insert(
        imported,
        GraphImport {
            alias: Some("stdlib".to_string()),
        },
    );
    to.symbols.insert(
        symbol_id,
        Symbol {
            name: "T".to_string(),
            ty: Some(TypeDesc::Int),
            default_value: Some(serde_json::json!(123)),
            meta: serde_json::json!({ "k": 1 }),
        },
    );
    if let Some(group) = to.groups.get_mut(&group_id) {
        group.color = Some("red".to_string());
    }
    if let Some(edge) = to.edges.get_mut(&edge_id) {
        edge.deletable = Some(true);
        edge.reconnectable = Some(crate::core::EdgeReconnectable::Endpoint(
            crate::core::EdgeReconnectableEndpoint::Target,
        ));
    }
    if let Some(port) = to.ports.get_mut(&out) {
        port.connectable = Some(false);
        port.ty = Some(TypeDesc::String);
        port.data = serde_json::json!({ "p": 1 });
    }
    if let Some(node) = to.nodes.get_mut(&a) {
        node.pos.x = 42.0;
        node.selectable = Some(false);
        node.draggable = Some(false);
        node.connectable = Some(false);
        node.deletable = Some(false);
        node.extent = Some(crate::core::NodeExtent::Rect {
            rect: CanvasRect {
                origin: CanvasPoint { x: 1.0, y: 2.0 },
                size: CanvasSize {
                    width: 3.0,
                    height: 4.0,
                },
            },
        });
        node.expand_parent = Some(true);
        node.hidden = true;
    }
    if let Some(note) = to.sticky_notes.get_mut(&note_id) {
        note.text = "M".to_string();
        note.rect.origin.x = 9.0;
        note.color = Some("yellow".to_string());
    }

    let tx1 = graph_diff(&from, &to);
    let tx2 = graph_diff(&from, &to);
    assert_eq!(
        serde_json::to_string(tx1.ops()).unwrap(),
        serde_json::to_string(tx2.ops()).unwrap(),
        "diff must be deterministic"
    );

    assert!(
        tx1.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetNodeSelectable { id, .. } if *id == a)),
        "diff must include node setter ops for changed fields"
    );
    assert!(
        tx1.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetNodeExtent { id, .. } if *id == a)),
        "diff must include node setter ops for changed fields"
    );
    assert!(
        tx1.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetNodeHidden { id, .. } if *id == a)),
        "diff must include node setter ops for changed fields"
    );
    assert!(
        !tx1.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveNode { id, .. } if *id == a)),
        "diff must not use destructive node removal for soft field changes"
    );

    assert!(
        tx1.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetGroupColor { id, .. } if *id == group_id)),
        "diff must prefer group setter ops over remove+add to preserve parent bindings"
    );
    assert!(
        !tx1.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveGroup { id, .. } if *id == group_id)),
        "diff must not remove groups for color-only changes"
    );

    assert!(
        tx1.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetStickyNoteText { id, .. } if *id == note_id)),
        "diff must use sticky note setter ops for field changes"
    );
    assert!(
        tx1.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetStickyNoteRect { id, .. } if *id == note_id)),
        "diff must use sticky note setter ops for field changes"
    );
    assert!(
        tx1.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetStickyNoteColor { id, .. } if *id == note_id)),
        "diff must use sticky note setter ops for field changes"
    );
    assert!(
        !tx1.ops().iter().any(|op| {
            matches!(op, GraphOp::RemoveStickyNote { id, .. } if *id == note_id)
                || matches!(op, GraphOp::AddStickyNote { id, .. } if *id == note_id)
        }),
        "diff must not fall back to remove+add for sticky note field changes"
    );

    assert!(
        tx1.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetPortConnectable { id, .. } if *id == out)),
        "diff must use port setter ops for soft fields"
    );
    assert!(
        tx1.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetPortType { id, .. } if *id == out)),
        "diff must use port setter ops for soft fields"
    );
    assert!(
        tx1.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetPortData { id, .. } if *id == out)),
        "diff must use port setter ops for soft fields"
    );
    assert!(
        !tx1.ops().iter().any(|op| {
            matches!(op, GraphOp::RemovePort { id, .. } if *id == out)
                || matches!(op, GraphOp::AddPort { id, .. } if *id == out)
        }),
        "diff must not fall back to remove+add for soft port changes"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx1).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );
}

#[test]
fn graph_diff_roundtrips_when_deleting_a_node_with_ports_and_edges() {
    let mut from = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    from.nodes.insert(a, make_node("core.a"));
    from.nodes.insert(b, make_node("core.b"));

    let out = PortId::from_u128(20);
    let inn = PortId::from_u128(21);
    from.ports
        .insert(out, make_port(a, "out", PortDirection::Out));
    from.ports
        .insert(inn, make_port(b, "in", PortDirection::In));
    from.nodes.get_mut(&a).unwrap().ports.push(out);
    from.nodes.get_mut(&b).unwrap().ports.push(inn);

    let edge_id = EdgeId::from_u128(456);
    from.edges.insert(
        edge_id,
        Edge {
            kind: EdgeKind::Data,
            from: out,
            to: inn,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let mut to = from.clone();
    to.nodes.remove(&b);
    to.ports.remove(&inn);
    to.edges.remove(&edge_id);

    let tx = graph_diff(&from, &to);
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveNode { id, .. } if *id == b)),
        "diff must use reversible RemoveNode for node deletion"
    );
    assert!(
        !tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemovePort { id, .. } if *id == inn)),
        "diff must not double-remove ports that are already removed by RemoveNode"
    );
    assert!(
        !tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveEdge { id, .. } if *id == edge_id)),
        "diff must not double-remove edges that are already removed by RemoveNode"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );
}

#[test]
fn graph_diff_roundtrips_when_deleting_a_port_with_incident_edges() {
    let mut from = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    from.nodes.insert(a, make_node("core.a"));
    from.nodes.insert(b, make_node("core.b"));

    let out = PortId::from_u128(30);
    let inn = PortId::from_u128(31);
    from.ports
        .insert(out, make_port(a, "out", PortDirection::Out));
    from.ports
        .insert(inn, make_port(b, "in", PortDirection::In));
    from.nodes.get_mut(&a).unwrap().ports.push(out);
    from.nodes.get_mut(&b).unwrap().ports.push(inn);

    let edge_id = EdgeId::from_u128(789);
    from.edges.insert(
        edge_id,
        Edge {
            kind: EdgeKind::Data,
            from: out,
            to: inn,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let mut to = from.clone();
    to.nodes.get_mut(&a).unwrap().ports.retain(|p| *p != out);
    to.ports.remove(&out);
    to.edges.remove(&edge_id);

    let tx = graph_diff(&from, &to);
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemovePort { id, .. } if *id == out)),
        "diff must use reversible RemovePort for port deletion"
    );
    assert!(
        !tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveEdge { id, .. } if *id == edge_id)),
        "diff must not double-remove edges that are already removed by RemovePort"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );
}

#[test]
fn graph_diff_roundtrips_when_a_port_changes_structurally() {
    let mut from = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    from.nodes.insert(a, make_node("core.a"));
    from.nodes.insert(b, make_node("core.b"));

    let out = PortId::from_u128(40);
    let inn = PortId::from_u128(41);
    from.ports
        .insert(out, make_port(a, "out", PortDirection::Out));
    from.ports
        .insert(inn, make_port(b, "in", PortDirection::In));
    from.nodes.get_mut(&a).unwrap().ports.push(out);
    from.nodes.get_mut(&b).unwrap().ports.push(inn);

    let edge_id = EdgeId::from_u128(1010);
    from.edges.insert(
        edge_id,
        Edge {
            kind: EdgeKind::Data,
            from: out,
            to: inn,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let mut to = from.clone();
    to.ports.get_mut(&out).unwrap().key = PortKey::new("out2");

    let tx = graph_diff(&from, &to);
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemovePort { id, .. } if *id == out)),
        "diff must represent structural port changes as remove+add"
    );
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::AddPort { id, .. } if *id == out)),
        "diff must represent structural port changes as remove+add"
    );
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetNodePorts { id, .. } if *id == a)),
        "diff must restore node port ordering after remove+add"
    );
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::AddEdge { id, .. } if *id == edge_id)),
        "diff must re-add incident edges removed by RemovePort when they still exist in 'to'"
    );
    assert!(
        !tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveEdge { id, .. } if *id == edge_id)),
        "diff must not double-remove edges that are removed by RemovePort"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );
}

#[test]
fn graph_diff_roundtrips_when_deleting_a_group_with_child_nodes() {
    let mut from = Graph::default();
    let group_id = GroupId::from_u128(10);
    from.groups.insert(
        group_id,
        Group {
            title: "Group".to_string(),
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

    let child_a = NodeId::from_u128(200);
    let child_b = NodeId::from_u128(100);
    let mut node_a = make_node("core.a");
    node_a.parent = Some(group_id);
    from.nodes.insert(child_a, node_a);
    let mut node_b = make_node("core.b");
    node_b.parent = Some(group_id);
    from.nodes.insert(child_b, node_b);

    from.nodes
        .insert(NodeId::from_u128(300), make_node("core.c"));

    let mut to = from.clone();
    to.groups.remove(&group_id);
    to.nodes.get_mut(&child_a).unwrap().parent = None;
    to.nodes.get_mut(&child_b).unwrap().parent = None;

    let tx = graph_diff(&from, &to);
    let expected_detached = vec![(child_b, Some(group_id)), (child_a, Some(group_id))];
    assert!(
        tx.ops().iter().any(|op| match op {
            GraphOp::RemoveGroup { id, detached, .. } if *id == group_id => {
                detached == &expected_detached
            }
            _ => false,
        }),
        "diff must use reversible RemoveGroup and detach child nodes deterministically"
    );
    assert!(
        !tx.ops().iter().any(|op| matches!(
            op,
            GraphOp::SetNodeParent { id, .. } if *id == child_a || *id == child_b
        )),
        "diff must not emit SetNodeParent when the parent group is being removed"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );
}

#[test]
fn graph_diff_roundtrips_when_edge_endpoints_change() {
    let mut from = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    from.nodes.insert(a, make_node("core.a"));
    from.nodes.insert(b, make_node("core.b"));

    let out1 = PortId::from_u128(50);
    let out2 = PortId::from_u128(51);
    let inn = PortId::from_u128(52);
    from.ports
        .insert(out1, make_port(a, "out1", PortDirection::Out));
    from.ports
        .insert(out2, make_port(a, "out2", PortDirection::Out));
    from.ports
        .insert(inn, make_port(b, "in", PortDirection::In));
    from.nodes.get_mut(&a).unwrap().ports.extend([out1, out2]);
    from.nodes.get_mut(&b).unwrap().ports.push(inn);

    let edge_id = EdgeId::from_u128(2020);
    from.edges.insert(
        edge_id,
        Edge {
            kind: EdgeKind::Data,
            from: out1,
            to: inn,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let mut to = from.clone();
    to.edges.get_mut(&edge_id).unwrap().from = out2;

    let tx = graph_diff(&from, &to);
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetEdgeEndpoints { id, .. } if *id == edge_id)),
        "diff must preserve edge identity when endpoints change"
    );
    assert!(
        !tx.ops().iter().any(|op| {
            matches!(op, GraphOp::RemoveEdge { id, .. } if *id == edge_id)
                || matches!(op, GraphOp::AddEdge { id, .. } if *id == edge_id)
        }),
        "diff must not fall back to remove+add for edge endpoint changes"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );
}
