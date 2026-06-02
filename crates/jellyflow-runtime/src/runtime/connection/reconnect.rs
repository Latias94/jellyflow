use serde::{Deserialize, Serialize};

use crate::rules::{
    ConnectPlan, Diagnostic, EdgeEndpoint, plan_reconnect_edge_with_mode_and_policy,
};
use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};
use jellyflow_core::core::{EdgeId, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::GraphTransaction;

/// Default transaction label used for committed reconnect updates.
pub const RECONNECT_EDGE_TRANSACTION_LABEL: &str = "reconnect edge";

/// Rules-driven request for reconnecting one endpoint of an existing edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReconnectEdgeRequest {
    pub edge: EdgeId,
    pub endpoint: EdgeEndpoint,
    pub new_port: PortId,
    #[serde(default)]
    pub mode: NodeGraphConnectionMode,
}

impl ReconnectEdgeRequest {
    pub fn new(
        edge: EdgeId,
        endpoint: EdgeEndpoint,
        new_port: PortId,
        mode: NodeGraphConnectionMode,
    ) -> Self {
        Self {
            edge,
            endpoint,
            new_port,
            mode,
        }
    }
}

/// Error returned when a reconnect request could not be committed.
#[derive(Debug, thiserror::Error)]
pub enum ReconnectEdgeError {
    #[error("reconnect edge was rejected")]
    Rejected { diagnostics: Vec<Diagnostic> },
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
}

impl ReconnectEdgeError {
    pub fn diagnostics(&self) -> Option<&[Diagnostic]> {
        match self {
            Self::Rejected { diagnostics } => Some(diagnostics),
            Self::Dispatch(_) => None,
        }
    }
}

pub fn reconnect_edge_transaction(plan: &ConnectPlan) -> Option<GraphTransaction> {
    if !plan.is_accept() || plan.ops().is_empty() {
        return None;
    }

    Some(
        GraphTransaction::from_ops(plan.ops().iter().cloned())
            .with_label(RECONNECT_EDGE_TRANSACTION_LABEL),
    )
}

fn reconnect_edge_transaction_from_plan(plan: ConnectPlan) -> Option<GraphTransaction> {
    if !plan.is_accept() || plan.ops().is_empty() {
        return None;
    }

    Some(GraphTransaction::from_ops(plan.into_ops()).with_label(RECONNECT_EDGE_TRANSACTION_LABEL))
}

impl NodeGraphStore {
    /// Plans reconnecting one endpoint of an existing edge against the resolved interaction policy.
    pub fn plan_reconnect_edge(&self, request: ReconnectEdgeRequest) -> ConnectPlan {
        let interaction = self.resolved_interaction_state();
        plan_reconnect_edge_with_mode_and_policy(
            self.graph(),
            request.edge,
            request.endpoint,
            request.new_port,
            request.mode,
            &interaction,
        )
    }

    /// Commits a reconnect request through normal store dispatch.
    pub fn apply_reconnect_edge(
        &mut self,
        request: ReconnectEdgeRequest,
    ) -> Result<Option<DispatchOutcome>, ReconnectEdgeError> {
        let plan = self.plan_reconnect_edge(request);
        if plan.is_reject() {
            return Err(ReconnectEdgeError::Rejected {
                diagnostics: plan.diagnostics,
            });
        }

        let Some(transaction) = reconnect_edge_transaction_from_plan(plan) else {
            return Ok(None);
        };

        self.dispatch_transaction(&transaction)
            .map(Some)
            .map_err(ReconnectEdgeError::from)
    }
}
