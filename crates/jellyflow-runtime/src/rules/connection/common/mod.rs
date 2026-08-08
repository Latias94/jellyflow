mod capacity;
mod edge_ops;
mod endpoints;
mod policy;
mod query;
mod rejections;
mod semantics;
mod type_check;

pub(super) use capacity::disconnect_for_endpoint_capacity_using;
pub(super) use edge_ops::{add_existing_ports_edge_op, connection_exists_using};
pub(super) use endpoints::{ConnectionEndpoints, resolve_ordered_connection_endpoints};
pub(super) use policy::{reject_if_connection_policy_disallows, resolve_policy_checked_connection};
pub(crate) use query::ConnectionPlanningContext;
pub(super) use rejections::{
    reject_duplicate_connection, reject_edge_kind_incompatible_with_ports,
    reject_edge_not_reconnectable, reject_missing_edge, reject_reconnect_directions_required,
    reject_self_connection,
};
pub(in crate::rules::connection) use semantics::{
    ConnectionOrder, port_kinds_are_compatible, resolve_connection_order,
    resolved_types_compatibility,
};
pub(crate) use semantics::{ConnectionPortFacts, connection_semantics_accepts};
pub(crate) use type_check::edge_type_compatibility_diagnostic;
pub(super) use type_check::type_compatibility_rejection;
