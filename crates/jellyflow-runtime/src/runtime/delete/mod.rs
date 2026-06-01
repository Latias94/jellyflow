//! Renderer-neutral delete-selection helpers.
//!
//! These helpers turn the current store selection plus keyboard policy into normal graph
//! transactions without depending on DOM keyboard events or renderer state.

mod planner;
mod store;
mod types;

pub use planner::{
    delete_selection_transaction, delete_selection_transaction_from_plan, plan_delete_selection,
    plan_delete_selection_for_key,
};
pub use types::{DELETE_SELECTION_TRANSACTION_LABEL, DeleteSelectionError};
