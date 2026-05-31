use jellyflow_core::core::{EdgeKind, PortKind};

pub(in crate::rules::connection) fn port_kind_for_edge_kind(edge_kind: EdgeKind) -> PortKind {
    match edge_kind {
        EdgeKind::Data => PortKind::Data,
        EdgeKind::Exec => PortKind::Exec,
    }
}

pub(in crate::rules::connection) fn edge_kind_for_port_kind(
    port_kind: PortKind,
) -> Option<EdgeKind> {
    match port_kind {
        PortKind::Data => Some(EdgeKind::Data),
        PortKind::Exec => Some(EdgeKind::Exec),
    }
}
