use std::collections::BTreeMap;

use jellyflow::core::{CanvasPoint, CanvasSize, Graph, GraphOp, GraphTransaction, NodeId};

/// Widget-free node transform snapshot observed by an Open GPUI host.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiNodeTransformSnapshot {
    pub node: NodeId,
    pub position: CanvasPoint,
    pub size: Option<CanvasSize>,
}

impl OpenGpuiNodeTransformSnapshot {
    pub fn new(node: NodeId, position: CanvasPoint) -> Self {
        Self {
            node,
            position,
            size: None,
        }
    }

    pub fn with_size(mut self, size: CanvasSize) -> Self {
        self.size = Some(size);
        self
    }
}

/// Plans a graph transaction that syncs host node transforms back to Jellyflow.
pub fn plan_transform_sync_transaction(
    graph: &Graph,
    snapshots: impl IntoIterator<Item = OpenGpuiNodeTransformSnapshot>,
) -> GraphTransaction {
    let snapshots = snapshots
        .into_iter()
        .map(|snapshot| (snapshot.node, snapshot))
        .collect::<BTreeMap<_, _>>();
    let mut transaction = GraphTransaction::new().with_label("sync open-gpui host node transforms");

    for (node_id, snapshot) in snapshots {
        let Some(node) = graph.nodes().get(&node_id) else {
            continue;
        };

        if snapshot.position.is_finite() && node.pos != snapshot.position {
            transaction.push(GraphOp::SetNodePos {
                id: node_id,
                from: node.pos,
                to: snapshot.position,
            });
        }

        let Some(size) = snapshot.size else {
            continue;
        };
        if size.is_positive_finite() && node.size != Some(size) {
            transaction.push(GraphOp::SetNodeSize {
                id: node_id,
                from: node.size,
                to: Some(size),
            });
        }
    }

    transaction
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::core::{GraphBuilder, GraphId, Node, NodeKindKey};

    #[test]
    fn transform_sync_updates_moved_and_resized_nodes_only() {
        let node_id = NodeId::from_u128(1);
        let unchanged_id = NodeId::from_u128(2);
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(node_id, node_at(10.0, 20.0, Some((120.0, 80.0))))
            .with_node(unchanged_id, node_at(40.0, 50.0, Some((90.0, 70.0))))
            .build_unchecked();

        let transaction = plan_transform_sync_transaction(
            &graph,
            [
                OpenGpuiNodeTransformSnapshot::new(node_id, CanvasPoint { x: 24.0, y: 36.0 })
                    .with_size(CanvasSize {
                        width: 140.0,
                        height: 96.0,
                    }),
                OpenGpuiNodeTransformSnapshot::new(unchanged_id, CanvasPoint { x: 40.0, y: 50.0 })
                    .with_size(CanvasSize {
                        width: 90.0,
                        height: 70.0,
                    }),
            ],
        );

        match transaction.ops() {
            [
                GraphOp::SetNodePos { id, to, .. },
                GraphOp::SetNodeSize {
                    id: size_id,
                    to: Some(size),
                    ..
                },
            ] => {
                assert_eq!(*id, node_id);
                assert_eq!(*to, CanvasPoint { x: 24.0, y: 36.0 });
                assert_eq!(*size_id, node_id);
                assert_eq!(
                    *size,
                    CanvasSize {
                        width: 140.0,
                        height: 96.0,
                    }
                );
            }
            ops => panic!("expected position and size ops for moved node only, got {ops:?}"),
        }
    }

    #[test]
    fn transform_sync_ignores_unknown_nodes_and_invalid_geometry() {
        let node_id = NodeId::from_u128(1);
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(node_id, node_at(10.0, 20.0, Some((120.0, 80.0))))
            .build_unchecked();

        let transaction = plan_transform_sync_transaction(
            &graph,
            [
                OpenGpuiNodeTransformSnapshot::new(
                    NodeId::from_u128(900),
                    CanvasPoint { x: 24.0, y: 36.0 },
                )
                .with_size(CanvasSize {
                    width: 140.0,
                    height: 96.0,
                }),
                OpenGpuiNodeTransformSnapshot::new(
                    node_id,
                    CanvasPoint {
                        x: f32::NAN,
                        y: 36.0,
                    },
                )
                .with_size(CanvasSize {
                    width: 0.0,
                    height: 96.0,
                }),
            ],
        );

        assert!(transaction.is_empty(), "{transaction:?}");
    }

    fn node_at(x: f32, y: f32, size: Option<(f32, f32)>) -> Node {
        Node {
            kind: NodeKindKey::new("demo.node"),
            kind_version: 1,
            pos: CanvasPoint { x, y },
            origin: None,
            selectable: None,
            focusable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: size.map(|(width, height)| CanvasSize { width, height }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::Value::Null,
        }
    }
}
