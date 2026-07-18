use jellyflow_core::core::{CanvasPoint, NodeId, PortId};

use crate::runtime::commands::{GraphCommand, NodeGeometryEdit};
use crate::runtime::graph_runtime::{GraphRevision, GraphSnapshot};

use super::ConnectionGestureIntent;

/// A graph mutation that must be bound to one fresh snapshot immediately before enqueueing.
#[derive(Debug)]
pub enum FreshGraphMutation {
    Connection {
        intent: ConnectionGestureIntent,
        target: PortId,
        validated_revision: GraphRevision,
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
            } => {
                if snapshot.revision() != validated_revision {
                    return Err(FreshGraphMutationBindError::RevisionChanged {
                        validated: validated_revision,
                        current: snapshot.revision(),
                    });
                }
                Ok(Some(GraphCommand::connection(
                    snapshot.revision(),
                    intent.edit_for_target(target),
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
        CanvasSize, EdgeId, GraphBuilder, GraphId, Node, NodeKindKey, PortId,
    };
    use jellyflow_core::interaction::NodeGraphConnectionMode;

    use super::*;
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
        };

        assert!(matches!(
            mutation.into_command(&runtime.snapshot().unwrap()),
            Err(FreshGraphMutationBindError::RevisionChanged {
                validated: 1,
                current: 0,
            })
        ));
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
}
