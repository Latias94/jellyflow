use crate::io::NodeGraphInteractionState;
use crate::rules::{ConnectPlan, Diagnostic, DiagnosticTarget};
use jellyflow_core::core::{EdgeKind, Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::{TypeCompatibility, TypeCompatibilityResult, TypeDesc};

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
    let endpoints = planned.endpoints();
    let edge_kind = endpoints.edge_kind;
    let from_id = endpoints.from_id;
    let to_id = endpoints.to_id;

    if edge_kind != EdgeKind::Data {
        return planned.into_plan();
    }

    let Some(from_ty) = type_of(graph, from_id) else {
        return planned.into_plan();
    };
    let Some(to_ty) = type_of(graph, to_id) else {
        return planned.into_plan();
    };

    match compat.compatible(&from_ty, &to_ty) {
        TypeCompatibilityResult::Compatible => planned.into_plan(),
        TypeCompatibilityResult::Incompatible { reason } => {
            ConnectPlan::reject_with_diagnostic(Diagnostic::error(
                "connect.type_mismatch",
                DiagnosticTarget::Port { id: to_id },
                format!("type mismatch: {reason} (from={from_ty:?} to={to_ty:?})"),
            ))
        }
    }
}
