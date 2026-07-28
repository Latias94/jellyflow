//! B-layer runtime building blocks.
//!
//! This module is intentionally **headless-safe**: it must not depend on `fret-ui`.
//! The runtime owns the graph command engine and explicit view-session protocol.

pub mod adapter_protocol;
pub mod commands;
pub mod geometry;
pub mod graph_runtime;
pub mod interaction;
pub mod middleware;
pub mod policy;
pub mod reconciliation;
pub(crate) mod transaction_gate;
mod unwind;
pub mod view_session;
pub mod viewport;

#[cfg(test)]
mod tests;
