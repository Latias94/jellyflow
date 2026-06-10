use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::xyflow::callbacks::{ConnectionChange, DeleteChange};
use crate::runtime::xyflow::changes::NodeGraphChanges;
use jellyflow_core::ops::GraphTransaction;

use super::connections::ConnectionChangeAccumulator;
use super::deletes::DeleteChangeAccumulator;
use super::node_graph::NodeGraphChangeAccumulator;

#[derive(Debug)]
pub(in crate::runtime::xyflow) struct XyFlowCommitProjection {
    node_edge_changes: NodeGraphChanges,
    connection_changes: Vec<ConnectionChange>,
    delete_change: DeleteChange,
}

impl XyFlowCommitProjection {
    pub(in crate::runtime::xyflow) fn from_patch(patch: &NodeGraphPatch) -> Self {
        Self::from_transaction(patch.transaction())
    }

    pub(in crate::runtime::xyflow) fn from_transaction(tx: &GraphTransaction) -> Self {
        let mut node_graph = NodeGraphChangeAccumulator::new();
        let mut connections = ConnectionChangeAccumulator::new(tx.len());
        let mut deletes = DeleteChangeAccumulator::default();

        for op in tx.ops() {
            node_graph.push_op(op);
            connections.push_op(op);
            deletes.push_op(op);
        }

        Self {
            node_edge_changes: node_graph.finish(),
            connection_changes: connections.finish(),
            delete_change: deletes.finish(),
        }
    }

    pub(in crate::runtime::xyflow) fn node_edge_changes(&self) -> &NodeGraphChanges {
        &self.node_edge_changes
    }

    pub(in crate::runtime::xyflow) fn connection_changes(&self) -> &[ConnectionChange] {
        &self.connection_changes
    }

    pub(in crate::runtime::xyflow) fn delete_change(&self) -> &DeleteChange {
        &self.delete_change
    }

    pub(in crate::runtime::xyflow) fn into_node_edge_changes(self) -> NodeGraphChanges {
        self.node_edge_changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::runtime::xyflow::callbacks::ConnectionChange;
    use crate::runtime::xyflow::changes::{EdgeChange, NodeChange};
    use jellyflow_core::core::{
        CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Group, GroupId, Node, NodeId,
        NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind, StickyNote,
        StickyNoteId,
    };
    use jellyflow_core::ops::{EdgeEndpoints, GraphOp};

    #[test]
    fn commit_projection_maps_edge_connection_ops() {
        let edge = TestEdgeFixture::new(PortId::new(), PortId::new());
        let unchanged = edge.endpoints();

        let tx = GraphTransaction::from_ops([
            GraphOp::AddEdge {
                id: edge.id,
                edge: edge.edge(),
            },
            GraphOp::SetEdgeEndpoints {
                id: edge.id,
                from: unchanged,
                to: unchanged,
            },
            GraphOp::RemoveEdge {
                id: edge.id,
                edge: edge.edge(),
            },
        ]);

        let projection = XyFlowCommitProjection::from_transaction(&tx);

        assert_eq!(projection.connection_changes().len(), 3);
        assert!(matches!(
            projection.connection_changes()[0],
            ConnectionChange::Connected(_)
        ));
        assert!(matches!(
            projection.connection_changes()[1],
            ConnectionChange::Reconnected { .. }
        ));
        assert!(matches!(
            projection.connection_changes()[2],
            ConnectionChange::Disconnected(_)
        ));
    }

    #[test]
    fn commit_projection_resets_connection_remove_dedup_after_edge_add() {
        let edge = TestEdgeFixture::new(PortId::new(), PortId::new());

        let tx = GraphTransaction::from_ops([
            GraphOp::RemoveEdge {
                id: edge.id,
                edge: edge.edge(),
            },
            GraphOp::AddEdge {
                id: edge.id,
                edge: edge.edge(),
            },
            GraphOp::RemoveEdge {
                id: edge.id,
                edge: edge.edge(),
            },
        ]);

        let projection = XyFlowCommitProjection::from_transaction(&tx);

        assert!(matches!(
            projection.connection_changes(),
            [
                ConnectionChange::Disconnected(first),
                ConnectionChange::Connected(connected),
                ConnectionChange::Disconnected(second),
            ] if first.edge == edge.id && connected.edge == edge.id && second.edge == edge.id
        ));
    }

    #[test]
    fn commit_projection_maps_resource_deletions_and_cascaded_edges() {
        let node = TestNodeFixture::new();
        let edge = TestEdgeFixture::new(node.port, PortId::new());
        let group = TestGroupFixture::new("group");
        let sticky_note = TestStickyNoteFixture::new("note");

        let tx = GraphTransaction::from_ops([
            GraphOp::RemoveNode {
                id: node.id,
                node: node.node(),
                ports: vec![(node.port, node.port())],
                edges: vec![(edge.id, edge.edge())],
            },
            GraphOp::RemovePort {
                id: node.port,
                port: node.port(),
                edges: vec![(edge.id, edge.edge())],
            },
            GraphOp::RemoveGroup {
                id: group.id,
                group: group.group(),
                detached: vec![(node.id, None)],
            },
            GraphOp::RemoveStickyNote {
                id: sticky_note.id,
                note: sticky_note.note(),
            },
        ]);

        let projection = XyFlowCommitProjection::from_transaction(&tx);
        let delete_change = projection.delete_change();

        assert!(
            projection
                .node_edge_changes()
                .nodes()
                .iter()
                .any(|change| matches!(change, NodeChange::Remove { id } if *id == node.id))
        );
        assert!(
            matches!(projection.node_edge_changes().edges(), [EdgeChange::Remove { id }] if *id == edge.id)
        );
        assert!(
            matches!(projection.connection_changes(), [ConnectionChange::Disconnected(conn)] if conn.edge == edge.id)
        );
        assert_eq!(delete_change.nodes(), &[node.id]);
        assert_eq!(delete_change.edges(), &[edge.id]);
        assert_eq!(delete_change.groups(), &[group.id]);
        assert_eq!(delete_change.sticky_notes(), &[sticky_note.id]);
    }

    struct TestNodeFixture {
        id: NodeId,
        port: PortId,
    }

    impl TestNodeFixture {
        fn new() -> Self {
            Self {
                id: NodeId::new(),
                port: PortId::new(),
            }
        }

        fn node(&self) -> Node {
            Node {
                kind: NodeKindKey::new("test.node"),
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
                size: None,
                hidden: false,
                collapsed: false,
                ports: vec![self.port],
                data: serde_json::Value::Null,
            }
        }

        fn port(&self) -> Port {
            Port {
                node: self.id,
                key: PortKey::new("out"),
                dir: PortDirection::Out,
                kind: PortKind::Data,
                capacity: PortCapacity::Multi,
                connectable: None,
                connectable_start: None,
                connectable_end: None,
                ty: None,
                data: serde_json::Value::Null,
            }
        }
    }

    struct TestEdgeFixture {
        id: EdgeId,
        from: PortId,
        to: PortId,
    }

    impl TestEdgeFixture {
        fn new(from: PortId, to: PortId) -> Self {
            Self {
                id: EdgeId::new(),
                from,
                to,
            }
        }

        fn edge(&self) -> Edge {
            Edge {
                kind: EdgeKind::Data,
                from: self.from,
                to: self.to,
                hidden: false,
                selectable: None,
                focusable: None,
                interaction_width: None,
                deletable: None,
                reconnectable: None,
            }
        }

        fn endpoints(&self) -> EdgeEndpoints {
            EdgeEndpoints {
                from: self.from,
                to: self.to,
            }
        }
    }

    struct TestGroupFixture {
        id: GroupId,
        title: &'static str,
    }

    impl TestGroupFixture {
        fn new(title: &'static str) -> Self {
            Self {
                id: GroupId::new(),
                title,
            }
        }

        fn group(&self) -> Group {
            Group {
                title: self.title.to_owned(),
                rect: test_rect(),
                color: None,
            }
        }
    }

    struct TestStickyNoteFixture {
        id: StickyNoteId,
        text: &'static str,
    }

    impl TestStickyNoteFixture {
        fn new(text: &'static str) -> Self {
            Self {
                id: StickyNoteId::new(),
                text,
            }
        }

        fn note(&self) -> StickyNote {
            StickyNote {
                text: self.text.to_owned(),
                rect: test_rect(),
                color: None,
            }
        }
    }

    fn test_rect() -> CanvasRect {
        CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        }
    }
}
