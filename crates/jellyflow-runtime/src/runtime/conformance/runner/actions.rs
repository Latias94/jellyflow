mod connection;
mod graph;
mod layout_facts;
mod node;
mod rendering;
mod selection;
mod viewport;

use crate::runtime::store::NodeGraphStore;

use super::super::scenario::ConformanceAction;

pub(super) fn execute_action(
    store: &mut NodeGraphStore,
    action: &ConformanceAction,
) -> Result<(), String> {
    if let Some(result) = graph::execute_action(store, action) {
        return result;
    }
    if let Some(result) = node::execute_action(store, action) {
        return result;
    }
    if let Some(result) = selection::execute_action(store, action) {
        return result;
    }
    if let Some(result) = connection::execute_action(store, action) {
        return result;
    }
    if let Some(result) = layout_facts::execute_action(store, action) {
        return result;
    }
    if let Some(result) = viewport::execute_action(store, action) {
        return result;
    }
    if let Some(result) = rendering::execute_action(store, action) {
        return result;
    }

    Err(format!("unhandled conformance action {}", action.kind()))
}

fn require_commit<T, E: ToString>(
    result: Result<Option<T>, E>,
    action: &'static str,
) -> Result<(), String> {
    match result {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(format!("{action} produced no commit")),
        Err(err) => Err(err.to_string()),
    }
}
