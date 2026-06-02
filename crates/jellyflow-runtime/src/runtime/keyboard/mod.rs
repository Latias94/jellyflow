//! Renderer-neutral keyboard intent helpers.
//!
//! This module collects store-facing keyboard actions such as deleting the current selection and
//! nudging selected nodes, so adapters do not need to route each key through unrelated modules.

mod store;
mod types;

pub use types::{KeyboardActionError, KeyboardActionOutcome, KeyboardDeleteAction, KeyboardIntent};
