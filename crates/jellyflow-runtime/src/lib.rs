//! Headless graph runtime, interaction protocol, schema, and persistence for Jellyflow.
//!
//! Every public concept has one owning module. Applications compose [`graph`], [`commands`],
//! [`view`], [`interaction`], [`geometry`], [`reconciliation`], and [`viewport`] without depending
//! on the private implementation tree or a UI framework.
//!
//! Root-level type aliases are intentionally absent:
//!
//! ```compile_fail
//! use jellyflow_runtime::GraphRuntime;
//! ```
//!
//! The implementation tree is not a second public entry point:
//!
//! ```compile_fail
//! use jellyflow_runtime::runtime::graph_runtime::GraphRuntime;
//! ```
//!
//! Raw rule plans and reversible operation journals are not public mutation surfaces:
//!
//! ```compile_fail
//! use jellyflow_runtime::rules::ConnectPlan;
//! ```

#![deny(unsafe_code)]

pub mod commands;
pub mod geometry;
pub mod graph;
mod graph_kernel;
pub mod interaction;
pub mod io;
pub mod profile;
mod rules;
mod runtime;
/// Pure, renderer-neutral reconciliation reducers for product-owned authoring.
pub mod reconciliation {
    pub use crate::runtime::reconciliation::*;
}
pub mod schema;
pub mod view;
pub mod viewport;
