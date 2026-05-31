use jellyflow_core::core::{EdgeKind, PortKind};

pub(in crate::rules::connection) fn port_kind_for_edge_kind(edge_kind: EdgeKind) -> PortKind {
    edge_kind.port_kind()
}

pub(in crate::rules::connection) fn edge_kind_for_port_kind(port_kind: PortKind) -> EdgeKind {
    port_kind.edge_kind()
}
