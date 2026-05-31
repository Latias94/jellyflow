mod edge_ops;
mod endpoints;
mod insert_spec;
mod kinds;
mod policy;

pub(super) use edge_ops::{
    ConnectionCapacity, add_existing_ports_edge_op, connection_exists, disconnect_for_capacity,
    edge_between, edge_like, ensure_edge_id_available, reject_mutation_error,
};
pub(super) use endpoints::{connection_ports, resolve_connection_endpoints};
pub(super) use insert_spec::validate_insert_node_spec;
pub(super) use kinds::{edge_kind_for_port_kind, port_kind_for_edge_kind};
pub(super) use policy::reject_if_connection_policy_disallows;
