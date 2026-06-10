mod edges;
mod nodes;

pub(in crate::runtime::xyflow) use edges::{
    apply_edge_update_change, edge_update_id, edge_update_op,
};
pub(in crate::runtime::xyflow) use nodes::{
    apply_node_update_change, node_update_id, node_update_op,
};
