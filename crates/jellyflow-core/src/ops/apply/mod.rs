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

use crate::core::Graph;
use crate::ops::GraphTransaction;

pub use error::ApplyError;

pub fn apply_transaction(graph: &mut Graph, tx: &GraphTransaction) -> Result<(), ApplyError> {
    dispatch::apply_transaction(graph, tx)
}
