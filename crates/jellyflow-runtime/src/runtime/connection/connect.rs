use serde::{Deserialize, Serialize};

use crate::rules::{ConnectPlan, Diagnostic, plan_connect_with_mode_and_policy};
use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};
use jellyflow_core::core::{EdgeId, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

/// Default transaction label used for committed connect updates.
pub const CONNECT_EDGE_TRANSACTION_LABEL: &str = "connect edge";

/// Rules-driven request for connecting two existing ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectEdgeRequest {
    pub from: PortId,
    pub to: PortId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<EdgeId>,
    #[serde(default)]
    pub mode: NodeGraphConnectionMode,
}

impl ConnectEdgeRequest {
    pub fn new(from: PortId, to: PortId, mode: NodeGraphConnectionMode) -> Self {
        Self {
            from,
            to,
            edge: None,
            mode,
        }
    }

    pub fn with_edge_id(mut self, edge: EdgeId) -> Self {
        self.edge = Some(edge);
        self
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
    connect_edge_transaction_with_optional_edge_id(plan, None)
}

pub fn connect_edge_transaction_with_edge_id(
    plan: &ConnectPlan,
    edge: EdgeId,
) -> Option<GraphTransaction> {
    connect_edge_transaction_with_optional_edge_id(plan, Some(edge))
}

fn connect_edge_transaction_with_optional_edge_id(
    plan: &ConnectPlan,
    edge: Option<EdgeId>,
) -> Option<GraphTransaction> {
    if !plan.is_accept() || plan.ops().is_empty() {
        return None;
    }

    Some(
        GraphTransaction::from_ops(connect_edge_ops(plan.ops().iter().cloned(), edge))
            .with_label(CONNECT_EDGE_TRANSACTION_LABEL),
    )
}

fn connect_edge_transaction_from_request(
    plan: ConnectPlan,
    request: ConnectEdgeRequest,
) -> Option<GraphTransaction> {
    if !plan.is_accept() || plan.ops().is_empty() {
        return None;
    }

    Some(
        GraphTransaction::from_ops(connect_edge_ops(plan.into_ops(), request.edge))
            .with_label(CONNECT_EDGE_TRANSACTION_LABEL),
    )
}

fn connect_edge_ops(
    ops: impl IntoIterator<Item = GraphOp>,
    edge: Option<EdgeId>,
) -> impl Iterator<Item = GraphOp> {
    ops.into_iter().map(move |op| match (edge, op) {
        (Some(id), GraphOp::AddEdge { edge, .. }) => GraphOp::AddEdge { id, edge },
        (_, op) => op,
    })
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

        let Some(transaction) = connect_edge_transaction_from_request(plan, request) else {
            return Ok(None);
        };

        self.dispatch_transaction(&transaction)
            .map(Some)
            .map_err(ConnectEdgeError::from)
    }
}
