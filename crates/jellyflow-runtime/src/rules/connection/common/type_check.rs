use crate::rules::{ConnectPlan, Diagnostic, DiagnosticTarget};
use jellyflow_core::core::{EdgeId, EdgeKind, Graph, PortId};
use jellyflow_core::types::{TypeCompatibility, TypeCompatibilityResult, TypeDesc};

use super::{ConnectionEndpoints, resolved_types_compatibility};

pub(in crate::rules::connection) fn type_compatibility_rejection(
    graph: &Graph,
    endpoints: &ConnectionEndpoints<'_>,
    type_of: &mut impl FnMut(&Graph, PortId) -> Option<TypeDesc>,
    compat: &dyn TypeCompatibility,
) -> Option<ConnectPlan> {
    let types = ConnectionTypePair::resolve(graph, endpoints, type_of)?;

    match resolved_types_compatibility(&types.from, &types.to, compat) {
        TypeCompatibilityResult::Compatible => None,
        TypeCompatibilityResult::Incompatible { reason } => Some(types.mismatch_plan(reason)),
    }
}

pub(crate) fn edge_type_compatibility_diagnostic(
    graph: &Graph,
    edge_id: EdgeId,
    mut type_of: impl FnMut(&Graph, PortId) -> Option<TypeDesc>,
    compat: &dyn TypeCompatibility,
) -> Option<Diagnostic> {
    let edge = graph.edge(&edge_id)?;
    if edge.kind != EdgeKind::Data {
        return None;
    }
    let from = type_of(graph, edge.from)?;
    let to = type_of(graph, edge.to)?;
    let TypeCompatibilityResult::Incompatible { reason } =
        resolved_types_compatibility(&from, &to, compat)
    else {
        return None;
    };
    Some(Diagnostic::error(
        "connect.type_mismatch",
        DiagnosticTarget::Port { id: edge.to },
        format!("type mismatch: {reason} (from={from:?} to={to:?})"),
    ))
}

struct ConnectionTypePair {
    target_id: PortId,
    from: TypeDesc,
    to: TypeDesc,
}

impl ConnectionTypePair {
    fn resolve(
        graph: &Graph,
        endpoints: &ConnectionEndpoints<'_>,
        type_of: &mut impl FnMut(&Graph, PortId) -> Option<TypeDesc>,
    ) -> Option<Self> {
        if endpoints.edge_kind != EdgeKind::Data {
            return None;
        }

        let from = type_of(graph, endpoints.from_id)?;
        let to = type_of(graph, endpoints.to_id)?;

        Some(Self {
            target_id: endpoints.to_id,
            from,
            to,
        })
    }

    fn mismatch_plan(self, reason: String) -> ConnectPlan {
        ConnectPlan::reject_with_diagnostic(Diagnostic::error(
            "connect.type_mismatch",
            DiagnosticTarget::Port { id: self.target_id },
            format!(
                "type mismatch: {reason} (from={:?} to={:?})",
                self.from, self.to
            ),
        ))
    }
}
