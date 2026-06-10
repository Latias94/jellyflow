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
        let out_port = PortId::new();
        let in_port = PortId::new();
        let edge_id = EdgeId::new();
        let edge = test_edge(out_port, in_port);

        let tx = GraphTransaction::from_ops([
            GraphOp::AddEdge {
                id: edge_id,
                edge: edge.clone(),
            },
            GraphOp::SetEdgeEndpoints {
                id: edge_id,
                from: EdgeEndpoints {
                    from: out_port,
                    to: in_port,
                },
                to: EdgeEndpoints {
                    from: out_port,
                    to: in_port,
                },
            },
            GraphOp::RemoveEdge { id: edge_id, edge },
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
        let out_port = PortId::new();
        let in_port = PortId::new();
        let edge_id = EdgeId::new();
        let edge = test_edge(out_port, in_port);

        let tx = GraphTransaction::from_ops([
            GraphOp::RemoveEdge {
                id: edge_id,
                edge: edge.clone(),
            },
            GraphOp::AddEdge {
                id: edge_id,
                edge: edge.clone(),
            },
            GraphOp::RemoveEdge { id: edge_id, edge },
        ]);

        let projection = XyFlowCommitProjection::from_transaction(&tx);

        assert!(matches!(
            projection.connection_changes(),
            [
                ConnectionChange::Disconnected(first),
                ConnectionChange::Connected(connected),
                ConnectionChange::Disconnected(second),
            ] if first.edge == edge_id && connected.edge == edge_id && second.edge == edge_id
        ));
    }

    #[test]
    fn commit_projection_maps_resource_deletions_and_cascaded_edges() {
        let node_id = NodeId::new();
        let out_port = PortId::new();
        let in_port = PortId::new();
        let edge_id = EdgeId::new();
        let group_id = GroupId::new();
        let note_id = StickyNoteId::new();
        let node = test_node(out_port);
        let port = test_port(node_id);
        let edge = test_edge(out_port, in_port);
        let rect = CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        };

        let tx = GraphTransaction::from_ops([
            GraphOp::RemoveNode {
                id: node_id,
                node,
                ports: vec![(out_port, port.clone())],
                edges: vec![(edge_id, edge.clone())],
            },
            GraphOp::RemovePort {
                id: out_port,
                port,
                edges: vec![(edge_id, edge)],
            },
            GraphOp::RemoveGroup {
                id: group_id,
                group: Group {
                    title: "group".to_owned(),
                    rect,
                    color: None,
                },
                detached: vec![(node_id, None)],
            },
            GraphOp::RemoveStickyNote {
                id: note_id,
                note: StickyNote {
                    text: "note".to_owned(),
                    rect,
                    color: None,
                },
            },
        ]);

        let projection = XyFlowCommitProjection::from_transaction(&tx);
        let delete_change = projection.delete_change();

        assert!(
            projection
                .node_edge_changes()
                .nodes()
                .iter()
                .any(|change| matches!(change, NodeChange::Remove { id } if *id == node_id))
        );
        assert!(
            matches!(projection.node_edge_changes().edges(), [EdgeChange::Remove { id }] if *id == edge_id)
        );
        assert!(
            matches!(projection.connection_changes(), [ConnectionChange::Disconnected(conn)] if conn.edge == edge_id)
        );
        assert_eq!(delete_change.nodes(), &[node_id]);
        assert_eq!(delete_change.edges(), &[edge_id]);
        assert_eq!(delete_change.groups(), &[group_id]);
        assert_eq!(delete_change.sticky_notes(), &[note_id]);
    }

    fn test_node(port: PortId) -> Node {
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
            ports: vec![port],
            data: serde_json::Value::Null,
        }
    }

    fn test_port(node: NodeId) -> Port {
        Port {
            node,
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

    fn test_edge(from: PortId, to: PortId) -> Edge {
        Edge {
            kind: EdgeKind::Data,
            from,
            to,
            hidden: false,
            selectable: None,
            focusable: None,
            interaction_width: None,
            deletable: None,
            reconnectable: None,
        }
    }
}
