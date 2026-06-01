//! Transaction projections for XyFlow-style callback payloads.

mod connections;
mod deletes;
mod node_graph;
mod removed_edges;

use crate::runtime::xyflow::callbacks::{ConnectionChange, DeleteChange};
use crate::runtime::xyflow::changes::NodeGraphChanges;
use jellyflow_core::ops::GraphTransaction;

pub(super) fn node_graph_changes_from_transaction(tx: &GraphTransaction) -> NodeGraphChanges {
    node_graph::node_graph_changes_from_transaction(tx)
}

pub(super) fn connection_changes_from_transaction(tx: &GraphTransaction) -> Vec<ConnectionChange> {
    connections::connection_changes_from_transaction(tx)
}

pub(super) fn delete_changes_from_transaction(tx: &GraphTransaction) -> DeleteChange {
    deletes::delete_changes_from_transaction(tx)
}
