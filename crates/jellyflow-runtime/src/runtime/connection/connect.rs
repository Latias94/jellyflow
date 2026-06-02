use serde::{Deserialize, Serialize};

use crate::rules::{ConnectPlan, Diagnostic, plan_connect_with_mode_and_policy};
use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};
use jellyflow_core::core::PortId;
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::GraphTransaction;

/// Default transaction label used for committed connect updates.
pub const CONNECT_EDGE_TRANSACTION_LABEL: &str = "connect edge";

/// Rules-driven request for connecting two existing ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectEdgeRequest {
    pub from: PortId,
    pub to: PortId,
    #[serde(default)]
    pub mode: NodeGraphConnectionMode,
}

impl ConnectEdgeRequest {
    pub fn new(from: PortId, to: PortId, mode: NodeGraphConnectionMode) -> Self {
        Self { from, to, mode }
    }
}

/// Error returned when a connect request could not be committed.
#[derive(Debug, thiserror::Error)]
pub enum ConnectEdgeError {
    #[error("connect edge was rejected")]
    Rejected { diagnostics: Vec<Diagnostic> },
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
}

impl ConnectEdgeError {
    pub fn diagnostics(&self) -> Option<&[Diagnostic]> {
        match self {
            Self::Rejected { diagnostics } => Some(diagnostics),
            Self::Dispatch(_) => None,
        }
    }
}

pub fn connect_edge_transaction(plan: &ConnectPlan) -> Option<GraphTransaction> {
    if !plan.is_accept() || plan.ops().is_empty() {
        return None;
    }

    Some(
        GraphTransaction::from_ops(plan.ops().iter().cloned())
            .with_label(CONNECT_EDGE_TRANSACTION_LABEL),
    )
}

fn connect_edge_transaction_from_plan(plan: ConnectPlan) -> Option<GraphTransaction> {
    if !plan.is_accept() || plan.ops().is_empty() {
        return None;
    }

    Some(GraphTransaction::from_ops(plan.into_ops()).with_label(CONNECT_EDGE_TRANSACTION_LABEL))
}

impl NodeGraphStore {
    /// Plans connecting two existing ports against the resolved interaction policy.
    pub fn plan_connect_edge(&self, request: ConnectEdgeRequest) -> ConnectPlan {
        let interaction = self.resolved_interaction_state();
        plan_connect_with_mode_and_policy(
            self.graph(),
            request.from,
            request.to,
            request.mode,
            &interaction,
        )
    }

    /// Commits a connect request through normal store dispatch.
    pub fn apply_connect_edge(
        &mut self,
        request: ConnectEdgeRequest,
    ) -> Result<Option<DispatchOutcome>, ConnectEdgeError> {
        let plan = self.plan_connect_edge(request);
        if plan.is_reject() {
            return Err(ConnectEdgeError::Rejected {
                diagnostics: plan.diagnostics,
            });
        }

        let Some(transaction) = connect_edge_transaction_from_plan(plan) else {
            return Ok(None);
        };

        self.dispatch_transaction(&transaction)
            .map(Some)
            .map_err(ConnectEdgeError::from)
    }
}
