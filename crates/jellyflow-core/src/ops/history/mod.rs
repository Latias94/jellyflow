//! Edit history (undo/redo) for graph transactions.
//!
//! The history stores committed (post-concretization) transactions and derives inverse transactions
//! by inverting operations in reverse order. It is intentionally headless and can be used by UI and
//! non-UI drivers.

mod invert;
mod store;

pub use invert::invert_transaction;
pub use store::{DEFAULT_HISTORY_LIMIT, GraphHistory};
