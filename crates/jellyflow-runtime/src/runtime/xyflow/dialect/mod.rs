mod edges;
mod nodes;

pub(in crate::runtime::xyflow) use edges::{
    edge_update_change_from_op, edge_update_id, edge_update_op,
};
pub(in crate::runtime::xyflow) use nodes::{
    node_update_change_from_op, node_update_id, node_update_op,
};
