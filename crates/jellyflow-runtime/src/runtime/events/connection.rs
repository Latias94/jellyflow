use serde::{Deserialize, Serialize};

use crate::rules::EdgeEndpoint;
use jellyflow_core::core::{EdgeId, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

/// Connection start kind (UI-driven).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConnectDragKind {
    New {
        from: PortId,
        bundle: Vec<PortId>,
    },
    Reconnect {
        edge: EdgeId,
        endpoint: EdgeEndpoint,
        fixed: PortId,
    },
    ReconnectMany {
        edges: Vec<(EdgeId, EdgeEndpoint, PortId)>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectStart {
    pub kind: ConnectDragKind,
    pub mode: NodeGraphConnectionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectEndOutcome {
    /// A graph transaction was committed.
    Committed,
    /// A target was chosen but the connect plan was rejected.
    Rejected,
    /// The workflow opened a conversion picker (domain-specific UX).
    OpenConversionPicker,
    /// The workflow opened an insert-node picker (drop on empty background).
    OpenInsertNodePicker,
    /// The gesture was canceled (escape, focus lost, etc.).
    Canceled,
    /// Gesture ended without committing or opening a picker.
    NoOp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectEnd {
    pub kind: ConnectDragKind,
    pub mode: NodeGraphConnectionMode,
    pub target: Option<PortId>,
    pub outcome: ConnectEndOutcome,
}
