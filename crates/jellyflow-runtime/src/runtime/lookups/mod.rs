//! Canonical lookup maps for fast graph queries (XyFlow-style).
//!
//! XyFlow maintains several "lookup maps" alongside its canonical node/edge arrays:
//! - `nodeLookup` (id -> internal node)
//! - `edgeLookup` (id -> edge)
//! - `connectionLookup` (node/handle -> connections)
//!
//! In Jellyflow the serialized document (`core::Graph`) is already map-based, but a first-class,
//! headless-safe lookup surface is still useful for:
//! - consistent adjacency queries (node/port -> incident edges),
//! - avoiding repeated full scans in editor shells,
//! - providing a stable substrate for B-layer tooling and middleware.

mod apply;
mod connections;
mod parents;
mod rebuild;
mod types;

use std::collections::HashMap;

use jellyflow_core::core::{EdgeId, NodeId};

pub use self::types::{
    ConnectionLookupKey, ConnectionSide, EdgeLookupEntry, HandleConnection, NodeLookupEntry,
};

#[derive(Debug, Default)]
pub struct NodeGraphLookups {
    pub node_lookup: HashMap<NodeId, NodeLookupEntry>,
    pub edge_lookup: HashMap<EdgeId, EdgeLookupEntry>,
    pub connection_lookup: HashMap<ConnectionLookupKey, HashMap<EdgeId, HandleConnection>>,
}
