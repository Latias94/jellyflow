//! B-layer runtime building blocks.
//!
//! This module is intentionally **headless-safe**: it must not depend on `fret-ui`.
//! The goal is to provide a stable, ergonomic "runtime/store" surface without coupling to
//! a specific rendering or widget layer. XyFlow-compatible projections live in [`xyflow`].

pub mod auto_pan;
pub mod commit;
pub mod conformance;
pub mod connection;
pub mod drag;
pub mod events;
pub mod fit_view;
pub mod geometry;
pub mod lookups;
pub mod middleware;
pub mod policy;
pub mod selection;
pub mod store;
pub mod utils;
pub mod viewport;
pub mod xyflow;

#[cfg(test)]
mod tests;
