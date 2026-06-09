//! Renderer-neutral delete-selection helpers.
//!
//! These helpers turn the current store selection plus keyboard policy into normal graph
//! transactions without depending on DOM keyboard events or renderer state.

mod planner;
mod store;
mod types;

pub use planner::{
    delete_elements_from_plan, delete_selection_elements, delete_selection_transaction,
    delete_selection_transaction_from_plan, plan_delete_elements, plan_delete_selection,
    plan_delete_selection_for_key, prepare_delete_elements, prepare_delete_selection,
    prepare_delete_selection_for_key,
};
pub use types::{
    DELETE_SELECTION_TRANSACTION_LABEL, DeleteElements, DeleteSelectionError, PreDeleteRequest,
    PreDeleteResolution,
};
