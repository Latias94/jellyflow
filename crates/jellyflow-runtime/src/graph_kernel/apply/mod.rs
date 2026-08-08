mod bindings;
mod dispatch;
mod edges;
mod error;
mod groups;
mod imports;
mod nodes;
mod ports;
mod resources;
mod sticky_notes;
mod symbols;

use crate::graph_kernel::GraphTransaction;
use jellyflow_core::core::Graph;

pub use error::ApplyError;

pub fn apply_transaction(graph: &mut Graph, tx: &GraphTransaction) -> Result<(), ApplyError> {
    dispatch::apply_transaction(graph, tx)
}

/// Applies a batch whose caller has already established every storage invariant.
///
/// Unlike [`apply_transaction`], this does not rescan the entire graph after the edit. The
/// operation sequence is still applied atomically: a failed operation leaves `graph` unchanged.
/// This is intended for authoritative mutation owners that construct exact reversible operations
/// from a validated snapshot.
pub fn apply_prevalidated_transaction_batch(
    graph: &mut Graph,
    tx: &GraphTransaction,
) -> Result<(), ApplyError> {
    dispatch::apply_prevalidated_transaction_batch(graph, tx)
}
