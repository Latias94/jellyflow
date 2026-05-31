mod edge_ops;
mod endpoints;
mod insert_spec;
mod kinds;
mod policy;

pub(super) use edge_ops::{
    add_existing_ports_edge_op, disconnect_for_capacity, edge_between, edge_like,
    reject_mutation_error,
};
pub(super) use endpoints::resolve_connection_endpoints;
pub(super) use insert_spec::validate_insert_node_spec;
pub(super) use kinds::{edge_kind_for_port_kind, port_kind_for_edge_kind};
pub(super) use policy::reject_if_connection_policy_disallows;
