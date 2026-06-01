mod edge_ops;
mod endpoints;
mod insert_spec;
mod policy;
mod rejections;

pub(super) use edge_ops::{
    ConnectionOpBuilder, add_existing_ports_edge_op, connection_exists, edge_between, edge_like,
    ensure_edge_id_available, reject_mutation_error,
};
pub(super) use endpoints::{ConnectionEndpoints, resolve_ordered_connection_endpoints};
pub(super) use insert_spec::validate_insert_node_spec;
pub(super) use policy::{reject_if_connection_policy_disallows, resolve_policy_checked_connection};
pub(super) use rejections::{
    reject_duplicate_connection, reject_edge_kind_incompatible,
    reject_edge_kind_incompatible_with_ports, reject_missing_edge,
    reject_reconnect_directions_required, reject_self_connection,
};
