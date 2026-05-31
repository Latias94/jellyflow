use crate::io::NodeGraphInteractionState;
use crate::rules::{
    ConnectDecision, ConnectPlan, Diagnostic, DiagnosticSeverity, DiagnosticTarget,
};
use jellyflow_core::core::{EdgeKind, Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::{TypeCompatibility, TypeCompatibilityResult, TypeDesc};

use super::common::resolve_connection_endpoints;
use super::connect::plan_connect_with_mode_and_policy;

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
    let base = plan_connect_with_mode_and_policy(graph, a, b, mode, state);
    if base.decision != ConnectDecision::Accept {
        return base;
    }

    let endpoints = match resolve_connection_endpoints(graph, a, b, mode) {
        Ok(endpoints) => endpoints,
        Err(plan) => return plan,
    };

    if endpoints.edge_kind != EdgeKind::Data {
        return base;
    }

    let Some(from_ty) = type_of(graph, endpoints.from_id) else {
        return base;
    };
    let Some(to_ty) = type_of(graph, endpoints.to_id) else {
        return base;
    };

    match compat.compatible(&from_ty, &to_ty) {
        TypeCompatibilityResult::Compatible => base,
        TypeCompatibilityResult::Incompatible { reason } => ConnectPlan {
            decision: ConnectDecision::Reject,
            diagnostics: vec![Diagnostic {
                key: "connect.type_mismatch".to_string(),
                severity: DiagnosticSeverity::Error,
                target: DiagnosticTarget::Port {
                    id: endpoints.to_id,
                },
                message: format!("type mismatch: {reason} (from={from_ty:?} to={to_ty:?})"),
                fixes: Vec::new(),
            }],
            ops: Vec::new(),
        },
    }
}
