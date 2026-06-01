use serde::{Deserialize, Serialize};

use jellyflow_core::core::GroupId;
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Node, NodeExtent, NodeId, NodeKindKey, PortId,
};

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
    Focusable {
        id: NodeId,
        focusable: Option<bool>,
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
