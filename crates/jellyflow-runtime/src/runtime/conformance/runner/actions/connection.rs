use crate::runtime::connection::{
    ConnectEdgeRequest, ConnectionTargetInput, ReconnectEdgeRequest, ResolvedConnectionTarget,
    resolve_connection_target, resolve_connection_target_from_handles,
};
use crate::runtime::store::NodeGraphStore;

use super::super::super::scenario::{
    ConformanceAction, ConformanceConnectionTargetFromHandlesInput,
};
use super::require_commit;

pub(super) fn execute_action(
    store: &mut NodeGraphStore,
    action: &ConformanceAction,
) -> Option<Result<(), String>> {
    Some(match action {
        ConformanceAction::AssertConnectionTarget { input, expected } => {
            assert_connection_target(*input, *expected)
        }
        ConformanceAction::AssertConnectionTargetFromHandles { input, expected } => {
            assert_connection_target_from_handles(input, *expected)
        }
        ConformanceAction::ApplyConnectEdge { request } => apply_connect_edge(store, *request),
        ConformanceAction::ApplyReconnectEdge { request } => apply_reconnect_edge(store, *request),
        _ => return None,
    })
}

pub(super) fn assert_connection_target(
    input: ConnectionTargetInput,
    expected: ResolvedConnectionTarget,
) -> Result<(), String> {
    let actual = resolve_connection_target(input);
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "connection target resolved to {actual:?}, expected {expected:?}"
        ))
    }
}

pub(super) fn assert_connection_target_from_handles(
    input: &ConformanceConnectionTargetFromHandlesInput,
    expected: ResolvedConnectionTarget,
) -> Result<(), String> {
    let actual = resolve_connection_target_from_handles(input.as_runtime());
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "connection target from handles resolved to {actual:?}, expected {expected:?}"
        ))
    }
}

pub(super) fn apply_connect_edge(
    store: &mut NodeGraphStore,
    request: ConnectEdgeRequest,
) -> Result<(), String> {
    require_commit(store.apply_connect_edge(request), "apply_connect_edge")
}

pub(super) fn apply_reconnect_edge(
    store: &mut NodeGraphStore,
    request: ReconnectEdgeRequest,
) -> Result<(), String> {
    require_commit(store.apply_reconnect_edge(request), "apply_reconnect_edge")
}
