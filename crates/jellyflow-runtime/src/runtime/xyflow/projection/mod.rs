//! Transaction projections for XyFlow-style callback payloads.

mod commit;
mod connections;
mod deletes;
mod node_graph;
mod removed_edges;

pub(in crate::runtime::xyflow) use commit::XyFlowCommitProjection;
