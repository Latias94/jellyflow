//! XyFlow-style change model for editor runtimes.
//!
//! In XyFlow/ReactFlow, internal interactions produce "changes" that user code can apply to its
//! node/edge arrays via helpers like `applyNodeChanges`. In Jellyflow, the authoritative model
//! is a reversible `GraphTransaction` (undo/redo friendly). This module bridges the two worlds:
//! - Map `GraphTransaction` -> `(NodeChange, EdgeChange)` events (for callbacks).
//! - Map `(NodeChange, EdgeChange)` -> reversible `GraphTransaction` (for store dispatch).
//!
//! These change names are compatibility vocabulary. Use [`crate::runtime::policy`] when an adapter
//! needs effective interaction policy such as whether a node can be selected or an edge endpoint can
//! be reconnected.

pub use crate::runtime::commit::NodeGraphPatch;

use serde::{Deserialize, Serialize};

use jellyflow_core::core::GroupId;
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, EdgeReconnectable, Graph, Node, NodeExtent,
    NodeId, NodeKindKey, PortId,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

/// Changes targeting nodes (graph-owned).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeChange {
    Add {
        id: NodeId,
        node: Node,
    },
    Remove {
        id: NodeId,
    },

    Position {
        id: NodeId,
        position: CanvasPoint,
    },
    Kind {
        id: NodeId,
        kind: NodeKindKey,
    },
    KindVersion {
        id: NodeId,
        kind_version: u32,
    },
    Selectable {
        id: NodeId,
        selectable: Option<bool>,
    },
    Draggable {
        id: NodeId,
        draggable: Option<bool>,
    },
    Connectable {
        id: NodeId,
        connectable: Option<bool>,
    },
    Deletable {
        id: NodeId,
        deletable: Option<bool>,
    },
    Parent {
        id: NodeId,
        parent: Option<GroupId>,
    },
    Extent {
        id: NodeId,
        extent: Option<NodeExtent>,
    },
    ExpandParent {
        id: NodeId,
        expand_parent: Option<bool>,
    },
    Size {
        id: NodeId,
        size: Option<CanvasSize>,
    },
    Hidden {
        id: NodeId,
        hidden: bool,
    },
    Collapsed {
        id: NodeId,
        collapsed: bool,
    },
    Data {
        id: NodeId,
        data: serde_json::Value,
    },
    Ports {
        id: NodeId,
        ports: Vec<PortId>,
    },
}

/// Changes targeting edges (graph-owned).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EdgeChange {
    Add {
        id: EdgeId,
        edge: Edge,
    },
    Remove {
        id: EdgeId,
    },

    Kind {
        id: EdgeId,
        kind: EdgeKind,
    },
    Selectable {
        id: EdgeId,
        selectable: Option<bool>,
    },
    Deletable {
        id: EdgeId,
        deletable: Option<bool>,
    },
    Reconnectable {
        id: EdgeId,
        reconnectable: Option<EdgeReconnectable>,
    },
    Endpoints {
        id: EdgeId,
        from: PortId,
        to: PortId,
    },
}

/// XyFlow-style node/edge projection of a graph patch.
///
/// This adapter is intentionally lossy: it only contains node and edge changes. Use
/// [`crate::runtime::commit::NodeGraphPatch`] when a consumer must observe full graph resources
/// such as ports, groups, sticky notes, imports, or symbols.
#[derive(Debug, Default, Clone)]
pub struct NodeGraphChanges {
    pub nodes: Vec<NodeChange>,
    pub edges: Vec<EdgeChange>,
}

#[derive(Debug, thiserror::Error)]
pub enum ChangesToTransactionError {
    #[error("node not found: {0:?}")]
    MissingNode(NodeId),
    #[error("edge not found: {0:?}")]
    MissingEdge(EdgeId),
}

impl NodeGraphChanges {
    pub fn from_patch(patch: &NodeGraphPatch) -> Self {
        Self::from_transaction(patch.transaction())
    }

    /// Derives change events from a reversible graph transaction.
    ///
    /// This is intended for XyFlow-style callbacks such as "on_nodes_change".
    pub fn from_transaction(tx: &GraphTransaction) -> Self {
        let mut out = Self::default();
        for op in &tx.ops {
            match op {
                GraphOp::AddNode { id, node } => out.nodes.push(NodeChange::Add {
                    id: *id,
                    node: node.clone(),
                }),
                GraphOp::RemoveNode { id, edges, .. } => {
                    out.nodes.push(NodeChange::Remove { id: *id });
                    for (edge_id, _edge) in edges {
                        out.edges.push(EdgeChange::Remove { id: *edge_id });
                    }
                }
                GraphOp::SetNodePos { id, to, .. } => out.nodes.push(NodeChange::Position {
                    id: *id,
                    position: *to,
                }),
                GraphOp::SetNodeKind { id, to, .. } => out.nodes.push(NodeChange::Kind {
                    id: *id,
                    kind: to.clone(),
                }),
                GraphOp::SetNodeKindVersion { id, to, .. } => {
                    out.nodes.push(NodeChange::KindVersion {
                        id: *id,
                        kind_version: *to,
                    })
                }
                GraphOp::SetNodeSelectable { id, to, .. } => {
                    out.nodes.push(NodeChange::Selectable {
                        id: *id,
                        selectable: *to,
                    })
                }
                GraphOp::SetNodeDraggable { id, to, .. } => out.nodes.push(NodeChange::Draggable {
                    id: *id,
                    draggable: *to,
                }),
                GraphOp::SetNodeConnectable { id, to, .. } => {
                    out.nodes.push(NodeChange::Connectable {
                        id: *id,
                        connectable: *to,
                    })
                }
                GraphOp::SetNodeDeletable { id, to, .. } => out.nodes.push(NodeChange::Deletable {
                    id: *id,
                    deletable: *to,
                }),
                GraphOp::SetNodeParent { id, to, .. } => out.nodes.push(NodeChange::Parent {
                    id: *id,
                    parent: *to,
                }),
                GraphOp::SetNodeExtent { id, to, .. } => out.nodes.push(NodeChange::Extent {
                    id: *id,
                    extent: *to,
                }),
                GraphOp::SetNodeExpandParent { id, to, .. } => {
                    out.nodes.push(NodeChange::ExpandParent {
                        id: *id,
                        expand_parent: *to,
                    })
                }
                GraphOp::SetNodeSize { id, to, .. } => {
                    out.nodes.push(NodeChange::Size { id: *id, size: *to })
                }
                GraphOp::SetNodeHidden { id, to, .. } => out.nodes.push(NodeChange::Hidden {
                    id: *id,
                    hidden: *to,
                }),
                GraphOp::SetNodeCollapsed { id, to, .. } => out.nodes.push(NodeChange::Collapsed {
                    id: *id,
                    collapsed: *to,
                }),
                GraphOp::SetNodeData { id, to, .. } => out.nodes.push(NodeChange::Data {
                    id: *id,
                    data: to.clone(),
                }),
                GraphOp::SetNodePorts { id, to, .. } => out.nodes.push(NodeChange::Ports {
                    id: *id,
                    ports: to.clone(),
                }),
                GraphOp::RemovePort { edges, .. } => {
                    for (edge_id, _edge) in edges {
                        out.edges.push(EdgeChange::Remove { id: *edge_id });
                    }
                }
                GraphOp::AddEdge { id, edge } => out.edges.push(EdgeChange::Add {
                    id: *id,
                    edge: edge.clone(),
                }),
                GraphOp::RemoveEdge { id, .. } => out.edges.push(EdgeChange::Remove { id: *id }),
                GraphOp::SetEdgeKind { id, to, .. } => {
                    out.edges.push(EdgeChange::Kind { id: *id, kind: *to })
                }
                GraphOp::SetEdgeSelectable { id, to, .. } => {
                    out.edges.push(EdgeChange::Selectable {
                        id: *id,
                        selectable: *to,
                    })
                }
                GraphOp::SetEdgeDeletable { id, to, .. } => out.edges.push(EdgeChange::Deletable {
                    id: *id,
                    deletable: *to,
                }),
                GraphOp::SetEdgeReconnectable { id, to, .. } => {
                    out.edges.push(EdgeChange::Reconnectable {
                        id: *id,
                        reconnectable: *to,
                    })
                }
                GraphOp::SetEdgeEndpoints { id, to, .. } => out.edges.push(EdgeChange::Endpoints {
                    id: *id,
                    from: to.from,
                    to: to.to,
                }),
                GraphOp::RemoveGroup { detached, .. } => {
                    for (node_id, _previous_parent) in detached {
                        out.nodes.push(NodeChange::Parent {
                            id: *node_id,
                            parent: None,
                        });
                    }
                }

                // These variants mutate graph resources that are outside the XyFlow-style
                // node/edge change-array contract. Full-fidelity controlled integrations should
                // apply the committed GraphTransaction from on_graph_commit.
                GraphOp::AddPort { .. }
                | GraphOp::SetPortConnectable { .. }
                | GraphOp::SetPortConnectableStart { .. }
                | GraphOp::SetPortConnectableEnd { .. }
                | GraphOp::SetPortType { .. }
                | GraphOp::SetPortData { .. }
                | GraphOp::AddImport { .. }
                | GraphOp::RemoveImport { .. }
                | GraphOp::SetImportAlias { .. }
                | GraphOp::AddSymbol { .. }
                | GraphOp::RemoveSymbol { .. }
                | GraphOp::SetSymbolName { .. }
                | GraphOp::SetSymbolType { .. }
                | GraphOp::SetSymbolDefaultValue { .. }
                | GraphOp::SetSymbolMeta { .. }
                | GraphOp::AddGroup { .. }
                | GraphOp::SetGroupRect { .. }
                | GraphOp::SetGroupTitle { .. }
                | GraphOp::SetGroupColor { .. }
                | GraphOp::AddStickyNote { .. }
                | GraphOp::RemoveStickyNote { .. }
                | GraphOp::SetStickyNoteText { .. }
                | GraphOp::SetStickyNoteRect { .. }
                | GraphOp::SetStickyNoteColor { .. } => {}
            }
        }
        out
    }

    /// Converts change events into a reversible transaction by looking up "from" values in the
    /// current graph.
    ///
    /// This enables an XyFlow-like runtime to accept `(NodeChange, EdgeChange)` and still keep
    /// `GraphHistory` undo/redo semantics.
    pub fn to_transaction(
        &self,
        graph: &Graph,
    ) -> Result<GraphTransaction, ChangesToTransactionError> {
        crate::runtime::xyflow::transaction::changes_to_transaction(self, graph)
    }
}
