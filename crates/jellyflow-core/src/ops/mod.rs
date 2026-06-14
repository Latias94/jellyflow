//! Undoable graph edit operations.

mod apply;
mod build;
mod diff;
mod fragment;
mod history;
mod mutation;
mod normalize;
mod transaction;
mod tx_sanity;

pub use apply::ApplyError;
pub use build::GraphOpBuilderExt;
pub use fragment::{GraphFragment, IdRemapSeed, IdRemapper, PasteTuning};
pub use history::{DEFAULT_HISTORY_LIMIT, GraphHistory};
pub use mutation::{
    GraphMutationBatchPlanner, GraphMutationError, GraphMutationPlanner, PortInsert,
};
pub use normalize::normalize_transaction;
pub use transaction::{EdgeEndpoints, GraphMutationFootprint, GraphOp, GraphTransaction};
pub use tx_sanity::{find_invalid_size_in_tx, find_non_finite_in_tx};

#[cfg(test)]
mod tests;
