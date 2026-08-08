//! Private reversible mutation and history kernel.

mod apply;
mod history;
mod mutation;
mod normalize;
mod transaction;
mod tx_sanity;

#[cfg(test)]
mod tests;

pub(crate) use apply::ApplyError;
pub(crate) use history::GraphHistory;
pub(crate) use jellyflow_core::GraphMutationFootprint;
pub(crate) use mutation::GraphMutationError;
pub(crate) use normalize::normalize_transaction;
pub(crate) use transaction::{EdgeEndpoints, GraphOp, GraphTransaction};
pub(crate) use tx_sanity::{
    find_invalid_edge_route_in_tx, find_invalid_size_in_tx, find_non_finite_in_tx,
};
