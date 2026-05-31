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
use jellyflow_core::ops::GraphTransaction;

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
        crate::runtime::xyflow::projection::node_graph_changes_from_transaction(tx)
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
