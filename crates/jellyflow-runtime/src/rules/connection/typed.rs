use crate::io::NodeGraphInteractionState;
use crate::rules::{ConnectPlan, Diagnostic, DiagnosticTarget};
use jellyflow_core::core::{EdgeKind, Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::{TypeCompatibility, TypeCompatibilityResult, TypeDesc};

use super::common::ConnectionEndpoints;
use super::connect::plan_resolved_connect;

/// Plans connecting two ports with optional type compatibility checks.
///
/// If both ports have a resolved type and the edge kind is `Data`, the `compat` policy is used to
/// enforce assignability (`from -> to`).
pub fn plan_connect_typed(
    graph: &Graph,
    a: PortId,
    b: PortId,
    type_of: impl FnMut(&Graph, PortId) -> Option<TypeDesc>,
    compat: &mut dyn TypeCompatibility,
) -> ConnectPlan {
    plan_connect_typed_with_policy(
        graph,
        a,
        b,
        &NodeGraphInteractionState::default(),
        type_of,
        compat,
    )
}

/// Plans connecting two ports with policy and optional type compatibility checks.
pub fn plan_connect_typed_with_policy(
    graph: &Graph,
    a: PortId,
    b: PortId,
    state: &NodeGraphInteractionState,
    type_of: impl FnMut(&Graph, PortId) -> Option<TypeDesc>,
    compat: &mut dyn TypeCompatibility,
) -> ConnectPlan {
    plan_connect_typed_with_mode_and_policy(
        graph,
        a,
        b,
        NodeGraphConnectionMode::Strict,
        state,
        type_of,
        compat,
    )
}

/// Plans connecting two ports with mode, policy, and optional type compatibility checks.
pub fn plan_connect_typed_with_mode_and_policy(
    graph: &Graph,
    a: PortId,
    b: PortId,
    mode: NodeGraphConnectionMode,
    state: &NodeGraphInteractionState,
    mut type_of: impl FnMut(&Graph, PortId) -> Option<TypeDesc>,
    compat: &mut dyn TypeCompatibility,
) -> ConnectPlan {
    let planned = match plan_resolved_connect(graph, a, b, mode, state) {
        Ok(planned) => planned,
        Err(plan) => return plan,
    };
    let Some(types) = ConnectionTypePair::resolve(graph, planned.endpoints(), &mut type_of) else {
        return planned.into_plan();
    };

    match compat.compatible(&types.from, &types.to) {
        TypeCompatibilityResult::Compatible => planned.into_plan(),
        TypeCompatibilityResult::Incompatible { reason } => types.mismatch_plan(reason),
    }
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
