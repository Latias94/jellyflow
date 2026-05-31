//! Headless helper utilities (XyFlow-style graph helpers).
//!
//! XyFlow exposes a set of convenience utilities in `@xyflow/system/src/utils/*` for common
//! graph queries (e.g. "incomers/outgoers", "connected edges", "nodes inside rect", bounds).
//!
//! In Jellyflow, the canonical document (`core::Graph`) is port-based (edges connect ports), so
//! these helpers are built on top of `runtime::lookups::NodeGraphLookups` which provides a stable,
//! headless-safe adjacency surface.

mod bounds;
mod connections;
mod options;

pub use bounds::{
    get_node_position_with_origin, get_node_rect, get_nodes_bounds, get_nodes_inside,
};
pub use connections::{
    get_connected_edges, get_connected_edges_for_nodes, get_incomers, get_outgoers,
};
pub use options::{GetNodesBoundsOptions, GetNodesInsideOptions, NodeInclusion};
