mod connection;
mod delete;

use super::super::traits::NodeGraphCallbacks;
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::xyflow::projection::XyFlowCommitProjection;

pub(super) fn dispatch_graph_commit_callbacks(
    callbacks: &mut dyn NodeGraphCallbacks,
    patch: &NodeGraphPatch,
) {
    let projection = XyFlowCommitProjection::from_patch(patch);
    let node_edge_changes = projection.node_edge_changes();
    callbacks.on_graph_commit(patch);
    callbacks.on_node_edge_changes(node_edge_changes);
    if !node_edge_changes.nodes().is_empty() {
        callbacks.on_nodes_change(node_edge_changes.nodes());
    }
    if !node_edge_changes.edges().is_empty() {
        callbacks.on_edges_change(node_edge_changes.edges());
    }

    connection::dispatch_connection_callbacks(callbacks, projection.connection_changes());
    delete::dispatch_delete_callbacks(callbacks, projection.delete_change());
}
