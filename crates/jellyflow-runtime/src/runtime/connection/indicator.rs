use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::{ConnectionHandleRef, ConnectionHandleValidity};

/// Input for resolving XyFlow-style handle interaction indicator state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionHandleIndicatorInput {
    pub handle: ConnectionHandleRef,
    pub from: Option<ConnectionHandleRef>,
    pub to: Option<ConnectionHandleRef>,
    pub click_start: Option<ConnectionHandleRef>,
    pub mode: NodeGraphConnectionMode,
    pub target_feedback: ConnectionHandleValidity,
    pub connectable: bool,
    pub connectable_start: bool,
    pub connectable_end: bool,
}

impl ConnectionHandleIndicatorInput {
    pub fn new(handle: ConnectionHandleRef, mode: NodeGraphConnectionMode) -> Self {
        Self {
            handle,
            from: None,
            to: None,
            click_start: None,
            mode,
            target_feedback: ConnectionHandleValidity::NoHandle,
            connectable: true,
            connectable_start: true,
            connectable_end: true,
        }
    }

    pub fn with_connection(
        mut self,
        from: Option<ConnectionHandleRef>,
        to: Option<ConnectionHandleRef>,
        target_feedback: ConnectionHandleValidity,
    ) -> Self {
        self.from = from;
        self.to = to;
        self.target_feedback = target_feedback;
        self
    }

    pub fn with_click_start(mut self, click_start: Option<ConnectionHandleRef>) -> Self {
        self.click_start = click_start;
        self
    }

    pub fn with_connectability(
        mut self,
        connectable: bool,
        connectable_start: bool,
        connectable_end: bool,
    ) -> Self {
        self.connectable = connectable;
        self.connectable_start = connectable_start;
        self.connectable_end = connectable_end;
        self
    }
}

/// XyFlow-style per-handle interaction indicator state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionHandleIndicator {
    pub connecting_from: bool,
    pub connecting_to: bool,
    pub click_connecting: bool,
    pub connection_in_progress: bool,
    pub click_connection_in_progress: bool,
    pub possible_end_handle: bool,
    pub valid: bool,
    pub show_connection_indicator: bool,
}

/// Resolves the handle booleans used by XyFlow's Handle component.
///
/// This mirrors the class-state logic while staying renderer-neutral. Adapters can map the result
/// to CSS classes, immediate-mode widget state, or GPU instance attributes.
pub fn resolve_connection_handle_indicator(
    input: ConnectionHandleIndicatorInput,
) -> ConnectionHandleIndicator {
    let connection_in_progress = input.from.is_some();
    let click_connection_in_progress = input.click_start.is_some();
    let connecting_from = input.from == Some(input.handle);
    let connecting_to = input.to == Some(input.handle);
    let click_connecting = input.click_start == Some(input.handle);
    let possible_end_handle = input.from.is_none_or(|from| match input.mode {
        NodeGraphConnectionMode::Strict => from.direction != input.handle.direction,
        NodeGraphConnectionMode::Loose => {
            from.node != input.handle.node || from.port != input.handle.port
        }
    });
    let valid = connecting_to && input.target_feedback == ConnectionHandleValidity::Valid;
    let show_connection_indicator = input.connectable
        && (!connection_in_progress || possible_end_handle)
        && if connection_in_progress || click_connection_in_progress {
            input.connectable_end
        } else {
            input.connectable_start
        };

    ConnectionHandleIndicator {
        connecting_from,
        connecting_to,
        click_connecting,
        connection_in_progress,
        click_connection_in_progress,
        possible_end_handle,
        valid,
        show_connection_indicator,
    }
}
