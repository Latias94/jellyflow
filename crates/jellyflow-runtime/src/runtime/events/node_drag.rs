use serde::{Deserialize, Serialize};

use jellyflow_core::core::{CanvasPoint, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeDragEndOutcome {
    Committed,
    Rejected,
    Canceled,
    NoOp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeDragStart {
    pub primary: NodeId,
    pub nodes: Vec<NodeId>,
    pub pointer: CanvasPoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeDragUpdate {
    pub primary: NodeId,
    pub nodes: Vec<NodeId>,
    pub pointer: CanvasPoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeDragEnd {
    pub primary: NodeId,
    pub nodes: Vec<NodeId>,
    pub pointer: CanvasPoint,
    pub outcome: NodeDragEndOutcome,
}
