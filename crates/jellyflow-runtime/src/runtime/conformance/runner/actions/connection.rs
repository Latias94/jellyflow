use crate::runtime::connection::{
    ConnectEdgeRequest, ConnectionEndIntent, ConnectionLifecycleResult, ConnectionTargetInput,
    ReconnectEdgeRequest, ResolvedConnectionTarget, resolve_connection_lifecycle,
    resolve_connection_target, resolve_connection_target_from_handles,
};
use crate::runtime::events::ConnectStart;
use crate::runtime::gesture::ConnectEdgeSession;
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
        ConformanceAction::AssertConnectionLifecycle {
            start,
            hover,
            intent,
            expected,
        } => assert_connection_lifecycle(start.clone(), *hover, *intent, expected.clone()),
        ConformanceAction::ApplyConnectEdge { request } => apply_connect_edge(store, *request),
        ConformanceAction::ApplyConnectEdgeSession { start, request } => {
            apply_connect_edge_session(store, start.clone(), *request)
        }
        ConformanceAction::ApplyReconnectEdge { request } => apply_reconnect_edge(store, *request),
        _ => return None,
    })
}

pub(super) fn assert_connection_lifecycle(
    start: ConnectStart,
    hover: Option<ResolvedConnectionTarget>,
    intent: ConnectionEndIntent,
    expected: ConnectionLifecycleResult,
) -> Result<(), String> {
    let actual = resolve_connection_lifecycle(start, hover, intent);
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "connection lifecycle resolved to {actual:?}, expected {expected:?}"
        ))
    }
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

pub(super) fn apply_connect_edge_session(
    store: &mut NodeGraphStore,
    start: ConnectStart,
    request: ConnectEdgeRequest,
) -> Result<(), String> {
    let outcome = store
        .apply_connect_edge_session(ConnectEdgeSession::new(start, request))
        .map_err(|err| err.to_string())?;
    if outcome.committed_update().is_some() {
        Ok(())
    } else {
        Err("apply_connect_edge_session produced no commit".to_owned())
    }
}

pub(super) fn apply_reconnect_edge(
    store: &mut NodeGraphStore,
    request: ReconnectEdgeRequest,
) -> Result<(), String> {
    require_commit(store.apply_reconnect_edge(request), "apply_reconnect_edge")
}
