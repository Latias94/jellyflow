use serde::{Deserialize, Serialize};

use crate::runtime::events::{ConnectDragKind, ConnectEnd, ConnectEndOutcome, ConnectStart};
use crate::schema::MenuDescriptor;
use jellyflow_core::core::{CanvasPoint, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::{ConnectionHandleConnection, ResolvedConnectionTarget};

/// Adapter-normalized intent for ending a connection gesture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConnectionEndIntent {
    /// End the gesture at the currently resolved hover target.
    Complete,
    /// Explicit cancellation: escape key, lost focus, tool switch, or adapter-level cancel action.
    Cancel,
    /// Pointer was released on the pane/background instead of on a handle.
    DropOnPane {
        pointer: CanvasPoint,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        menu: Option<MenuDescriptor>,
    },
}

/// Stable high-level connection lifecycle state for adapter presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionLifecycleState {
    Started,
    HoverValid,
    HoverInvalid,
    HoverEmpty,
    Committed,
    Rejected,
    Canceled,
    DroppedOnPane,
    NoOp,
}

/// Adapter-facing connection lifecycle result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionLifecycleResult {
    pub start: ConnectStart,
    pub state: ConnectionLifecycleState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<ResolvedConnectionTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionHandleConnection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<PortId>,
    pub end: ConnectEnd,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dropped_at: Option<CanvasPoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dropped_wire_menu: Option<MenuDescriptor>,
}

impl ConnectionLifecycleResult {
    pub fn for_hover(start: ConnectStart, hover: Option<ResolvedConnectionTarget>) -> Self {
        let state = match hover {
            Some(hover) if hover.is_handle_valid => ConnectionLifecycleState::HoverValid,
            Some(hover) if hover.target.is_some() => ConnectionLifecycleState::HoverInvalid,
            _ => ConnectionLifecycleState::HoverEmpty,
        };

        Self {
            end: ConnectEnd {
                kind: start.kind.clone(),
                mode: start.mode,
                target: hover.and_then(|hover| hover.target.map(|target| target.handle.port)),
                outcome: ConnectEndOutcome::NoOp,
            },
            start,
            state,
            hover,
            connection: hover.and_then(|hover| hover.connection),
            target: hover.and_then(|hover| hover.target.map(|target| target.handle.port)),
            dropped_at: None,
            dropped_wire_menu: None,
        }
    }

    pub fn did_commit(&self) -> bool {
        matches!(self.state, ConnectionLifecycleState::Committed)
    }

    pub fn opens_dropped_wire_menu(&self) -> bool {
        matches!(self.state, ConnectionLifecycleState::DroppedOnPane)
    }
}

/// Resolves a connection gesture lifecycle result from renderer-normalized inputs.
pub fn resolve_connection_lifecycle(
    start: ConnectStart,
    hover: Option<ResolvedConnectionTarget>,
    intent: ConnectionEndIntent,
) -> ConnectionLifecycleResult {
    let (state, connection, target, outcome, dropped_at, dropped_wire_menu) = match intent {
        ConnectionEndIntent::Cancel => (
            ConnectionLifecycleState::Canceled,
            None,
            None,
            ConnectEndOutcome::Canceled,
            None,
            None,
        ),
        ConnectionEndIntent::DropOnPane { pointer, menu } => (
            ConnectionLifecycleState::DroppedOnPane,
            None,
            None,
            ConnectEndOutcome::OpenInsertNodePicker,
            Some(pointer),
            menu,
        ),
        ConnectionEndIntent::Complete => resolve_completed_lifecycle(hover),
    };

    let end = ConnectEnd {
        kind: start.kind.clone(),
        mode: start.mode,
        target,
        outcome,
    };

    ConnectionLifecycleResult {
        start,
        state,
        hover,
        connection,
        target,
        end,
        dropped_at,
        dropped_wire_menu,
    }
}

fn resolve_completed_lifecycle(
    hover: Option<ResolvedConnectionTarget>,
) -> (
    ConnectionLifecycleState,
    Option<ConnectionHandleConnection>,
    Option<PortId>,
    ConnectEndOutcome,
    Option<CanvasPoint>,
    Option<MenuDescriptor>,
) {
    let Some(hover) = hover else {
        return (
            ConnectionLifecycleState::NoOp,
            None,
            None,
            ConnectEndOutcome::NoOp,
            None,
            None,
        );
    };

    if hover.is_handle_valid {
        let connection = hover.connection;
        let target = connection.map(|connection| connection.target.port);
        return (
            ConnectionLifecycleState::Committed,
            connection,
            target,
            ConnectEndOutcome::Committed,
            None,
            None,
        );
    }

    let target = hover.target.map(|target| target.handle.port);
    let state = if hover.target.is_some() {
        ConnectionLifecycleState::Rejected
    } else {
        ConnectionLifecycleState::NoOp
    };
    let outcome = if hover.target.is_some() {
        ConnectEndOutcome::Rejected
    } else {
        ConnectEndOutcome::NoOp
    };

    (state, hover.connection, target, outcome, None, None)
}

/// Creates a lifecycle start payload for a new edge gesture.
pub fn new_connection_start(
    from: PortId,
    bundle: impl IntoIterator<Item = PortId>,
    mode: NodeGraphConnectionMode,
) -> ConnectStart {
    ConnectStart {
        kind: ConnectDragKind::New {
            from,
            bundle: bundle.into_iter().collect(),
        },
        mode,
    }
}
