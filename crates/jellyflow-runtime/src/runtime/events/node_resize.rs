use serde::{Deserialize, Serialize};

use crate::runtime::resize::NodeResizeDirection;
use jellyflow_core::core::{CanvasPoint, CanvasSize, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeResizeEndOutcome {
    Committed,
    Rejected,
    Canceled,
    NoOp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeResizeStart {
    pub node: NodeId,
    pub direction: NodeResizeDirection,
    pub pointer: CanvasPoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeResizeUpdate {
    pub node: NodeId,
    pub direction: NodeResizeDirection,
    pub pointer: CanvasPoint,
    pub position: CanvasPoint,
    pub size: CanvasSize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeResizeEnd {
    pub node: NodeId,
    pub direction: NodeResizeDirection,
    pub pointer: CanvasPoint,
    pub outcome: NodeResizeEndOutcome,
}
