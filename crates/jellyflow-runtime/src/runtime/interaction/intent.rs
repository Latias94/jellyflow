use std::sync::Arc;

use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId, PortId};

use crate::runtime::commands::{GraphCommand, NodeGeometryEdit, TopologyEdit};
use crate::runtime::graph_runtime::{GraphRevision, GraphSnapshot};

use super::ConnectionGestureIntent;

/// The immutable selection observed when a selection gesture starts.
///
/// Marquee updates carry this baseline so every frame describes the selection relative to the
/// same authoritative starting point rather than incrementally mutating the previous frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionBaseline {
    nodes: Arc<[NodeId]>,
    edges: Arc<[EdgeId]>,
    groups: Arc<[GroupId]>,
}

impl SelectionBaseline {
    pub fn new(
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
        groups: impl IntoIterator<Item = GroupId>,
    ) -> Self {
        Self {
            nodes: nodes.into_iter().collect::<Vec<_>>().into(),
            edges: edges.into_iter().collect::<Vec<_>>().into(),
            groups: groups.into_iter().collect::<Vec<_>>().into(),
        }
    }

    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    pub fn edges(&self) -> &[EdgeId] {
        &self.edges
    }

    pub fn groups(&self) -> &[GroupId] {
        &self.groups
    }
}

/// Immutable topology retained while a connection command waits for freshness and settlement.
///
/// Cloning this value is cheap. The underlying edit is materialized only when a graph command is
/// bound, allowing create-from-wire commits to survive an adapter command-prefix barrier without
/// rebuilding application-owned node data.
#[derive(Debug, Clone, Default)]
pub struct ConnectionTopology(Arc<TopologyEdit>);

impl ConnectionTopology {
    pub fn ordinary() -> Self {
        Self::default()
    }

    pub fn new(edit: TopologyEdit) -> Self {
        Self(Arc::new(edit))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn edit(&self) -> &TopologyEdit {
        &self.0
    }

    fn into_edit(self) -> TopologyEdit {
        Arc::try_unwrap(self.0).unwrap_or_else(|shared| shared.as_ref().clone())
    }
}

/// A graph mutation that must be bound to one fresh snapshot immediately before enqueueing.
#[derive(Debug)]
pub enum FreshGraphMutation {
    Connection {
        intent: ConnectionGestureIntent,
        target: PortId,
        validated_revision: GraphRevision,
        topology: ConnectionTopology,
    },
    MoveNodes(RelativeNodeMoveIntent),
}

impl FreshGraphMutation {
    /// Consumes this intent and binds it once to the supplied authoritative snapshot.
    pub fn into_command(
        self,
        snapshot: &GraphSnapshot,
    ) -> Result<Option<GraphCommand>, FreshGraphMutationBindError> {
        match self {
            Self::Connection {
                intent,
                target,
                validated_revision,
                topology,
            } => {
                if snapshot.revision() != validated_revision {
                    return Err(FreshGraphMutationBindError::RevisionChanged {
                        validated: validated_revision,
                        current: snapshot.revision(),
                    });
                }
                Ok(Some(GraphCommand::topology(
                    snapshot.revision(),
                    topology
                        .into_edit()
                        .connection(intent.edit_for_target(target)),
                )))
            }
            Self::MoveNodes(intent) => Ok(intent.into_command(snapshot)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FreshGraphMutationBindError {
    #[error(
        "connection target was validated at revision {validated}, but the fresh snapshot is revision {current}"
    )]
    RevisionChanged {
        validated: GraphRevision,
        current: GraphRevision,
    },
}

/// Relative node movement captured independently from any stale press-time graph snapshot.
#[derive(Debug)]
pub struct RelativeNodeMoveIntent {
    nodes: Box<[NodeId]>,
    delta: CanvasPoint,
}

impl RelativeNodeMoveIntent {
    pub fn new(nodes: impl IntoIterator<Item = NodeId>, delta: CanvasPoint) -> Option<Self> {
        if !delta.is_finite() || (delta.x == 0.0 && delta.y == 0.0) {
            return None;
        }
        let mut nodes = nodes.into_iter().collect::<Vec<_>>();
        nodes.sort_unstable();
        nodes.dedup();
        (!nodes.is_empty()).then_some(Self {
            nodes: nodes.into_boxed_slice(),
            delta,
        })
    }

    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    pub fn delta(&self) -> CanvasPoint {
        self.delta
    }

    fn into_command(self, snapshot: &GraphSnapshot) -> Option<GraphCommand> {
        let edits = self
            .nodes
            .iter()
            .filter_map(|node| {
                let current = snapshot.graph().node(node)?;
                if !current.draggable.unwrap_or(true) {
                    return None;
                }
                let position = CanvasPoint {
                    x: current.pos.x + self.delta.x,
                    y: current.pos.y + self.delta.y,
                };
                position
                    .is_finite()
                    .then_some(NodeGeometryEdit::new(*node).with_position(position))
            })
            .collect::<Vec<_>>();
        (!edits.is_empty()).then(|| {
            GraphCommand::set_node_geometry(snapshot.revision(), edits).with_label("Move nodes")
        })
    }
}

#[cfg(test)]
mod tests {
    use jellyflow_core::core::{
        CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Node, NodeKindKey, Port,
        PortCapacity, PortDirection, PortId, PortKey, PortKind,
    };
    use jellyflow_core::interaction::NodeGraphConnectionMode;

    use super::*;
    use crate::rules::EdgeEndpoint;
    use crate::runtime::commands::TopologyPort;
    use crate::runtime::graph_runtime::GraphRuntime;

    #[test]
    fn relative_move_binds_once_to_fresh_positions_instead_of_press_time_positions() {
        let node = NodeId::from_u128(1);
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(node, test_node(CanvasPoint { x: 1.0, y: 2.0 }))
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let intent = RelativeNodeMoveIntent::new([node], CanvasPoint { x: 5.0, y: -1.0 })
            .expect("relative intent");
        runtime
            .execute(GraphCommand::set_node_position(
                0,
                node,
                CanvasPoint { x: 20.0, y: 30.0 },
            ))
            .unwrap();
        let fresh = runtime.snapshot().unwrap();

        let command = FreshGraphMutation::MoveNodes(intent)
            .into_command(&fresh)
            .unwrap()
            .expect("one movable node");
        assert_eq!(command.expected_revision(), 1);
        runtime.execute(command).unwrap();

        assert_eq!(
            runtime.snapshot().unwrap().graph().node(&node).unwrap().pos,
            CanvasPoint { x: 25.0, y: 29.0 }
        );
    }

    #[test]
    fn connection_bind_rejects_a_snapshot_newer_than_the_validated_probe() {
        let runtime = GraphRuntime::builder(Default::default())
            .build(GraphBuilder::new(GraphId::from_u128(1)).build().unwrap())
            .unwrap();
        let mutation = FreshGraphMutation::Connection {
            intent: ConnectionGestureIntent::connect(
                EdgeId::from_u128(1),
                PortId::from_u128(1),
                NodeGraphConnectionMode::Strict,
            ),
            target: PortId::from_u128(2),
            validated_revision: 1,
            topology: ConnectionTopology::ordinary(),
        };

        assert!(matches!(
            mutation.into_command(&runtime.snapshot().unwrap()),
            Err(FreshGraphMutationBindError::RevisionChanged {
                validated: 1,
                current: 0,
            })
        ));
    }

    #[test]
    fn create_from_wire_binds_node_and_connection_as_one_undoable_topology_command() {
        let source_node = NodeId::from_u128(1);
        let source_port = PortId::from_u128(1);
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let edge = EdgeId::from_u128(1);
        let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
        source.ports = vec![source_port];
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(source_node, source)
            .with_port(
                source_port,
                test_port(source_node, "out", PortDirection::Out),
            )
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let topology = TopologyEdit::new().add_node(
            target_node,
            test_node(CanvasPoint { x: 200.0, y: 0.0 }),
            [TopologyPort::new(
                target_port,
                test_port(target_node, "in", PortDirection::In),
            )],
        );
        let mutation = FreshGraphMutation::Connection {
            intent: ConnectionGestureIntent::connect(
                edge,
                source_port,
                NodeGraphConnectionMode::Strict,
            ),
            target: target_port,
            validated_revision: 0,
            topology: ConnectionTopology::new(topology),
        };

        let command = mutation
            .into_command(&runtime.snapshot().unwrap())
            .unwrap()
            .expect("atomic create-from-wire command");
        assert_eq!(command.expected_revision(), 0);
        runtime
            .execute(command)
            .unwrap()
            .applied()
            .expect("create-from-wire applies");
        let applied = runtime.snapshot().unwrap();
        assert!(applied.graph().node(&target_node).is_some());
        assert!(applied.graph().port(&target_port).is_some());
        assert!(applied.graph().edge(&edge).is_some());

        runtime
            .execute(GraphCommand::undo(1))
            .unwrap()
            .applied()
            .expect("one undo reverts the atomic command");
        let undone = runtime.snapshot().unwrap();
        assert!(undone.graph().node(&target_node).is_none());
        assert!(undone.graph().port(&target_port).is_none());
        assert!(undone.graph().edge(&edge).is_none());
        assert!(undone.graph().node(&source_node).is_some());
    }

    #[test]
    fn reconnect_from_wire_keeps_edge_identity_and_replaces_only_the_detached_endpoint() {
        let source_node = NodeId::from_u128(1);
        let source_port = PortId::from_u128(1);
        let old_target_node = NodeId::from_u128(2);
        let old_target_port = PortId::from_u128(2);
        let new_target_node = NodeId::from_u128(3);
        let new_target_port = PortId::from_u128(3);
        let edge = EdgeId::from_u128(1);
        let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
        source.ports = vec![source_port];
        let mut old_target = test_node(CanvasPoint { x: 200.0, y: 0.0 });
        old_target.ports = vec![old_target_port];
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(source_node, source)
            .with_port(
                source_port,
                test_port(source_node, "out", PortDirection::Out),
            )
            .with_node(old_target_node, old_target)
            .with_port(
                old_target_port,
                test_port(old_target_node, "old-in", PortDirection::In),
            )
            .with_edge(
                edge,
                Edge::new(EdgeKind::Data, source_port, old_target_port),
            )
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let topology = TopologyEdit::new().add_node(
            new_target_node,
            test_node(CanvasPoint { x: 400.0, y: 0.0 }),
            [TopologyPort::new(
                new_target_port,
                test_port(new_target_node, "new-in", PortDirection::In),
            )],
        );
        let mutation = FreshGraphMutation::Connection {
            intent: ConnectionGestureIntent::reconnect(
                edge,
                EdgeEndpoint::To,
                old_target_port,
                source_port,
                NodeGraphConnectionMode::Strict,
            ),
            target: new_target_port,
            validated_revision: 0,
            topology: ConnectionTopology::new(topology),
        };

        let command = mutation
            .into_command(&runtime.snapshot().unwrap())
            .unwrap()
            .expect("atomic reconnect-from-wire command");
        runtime
            .execute(command)
            .unwrap()
            .applied()
            .expect("reconnect-from-wire applies");
        let applied = runtime.snapshot().unwrap();
        assert_eq!(applied.graph().edges().len(), 1);
        let reconnected = applied.graph().edge(&edge).expect("stable edge identity");
        assert_eq!(reconnected.from, source_port);
        assert_eq!(reconnected.to, new_target_port);

        runtime
            .execute(GraphCommand::undo(applied.revision()))
            .unwrap()
            .applied()
            .expect("one undo restores reconnect-from-wire");
        let undone = runtime.snapshot().unwrap();
        assert!(undone.graph().node(&new_target_node).is_none());
        assert_eq!(undone.graph().edges().len(), 1);
        let restored = undone.graph().edge(&edge).expect("restored edge identity");
        assert_eq!(restored.from, source_port);
        assert_eq!(restored.to, old_target_port);
    }

    fn test_node(pos: CanvasPoint) -> Node {
        Node {
            kind: NodeKindKey::new("test.node"),
            kind_version: 1,
            pos,
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 160.0,
                height: 96.0,
            }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::Value::Null,
        }
    }

    fn test_port(node: NodeId, key: &str, dir: PortDirection) -> Port {
        Port {
            node,
            key: PortKey::new(key),
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
}
