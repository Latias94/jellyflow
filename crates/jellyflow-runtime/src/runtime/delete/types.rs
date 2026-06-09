use serde::{Deserialize, Serialize};

use crate::rules::Diagnostic;
use crate::runtime::store::DispatchError;
use jellyflow_core::core::{EdgeId, NodeId};

/// Default transaction label used for committed delete-selection updates.
pub const DELETE_SELECTION_TRANSACTION_LABEL: &str = "delete selection";

/// Node/edge ids participating in a delete request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteElements {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<EdgeId>,
}

impl DeleteElements {
    pub fn new(
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
    ) -> Self {
        let mut elements = Self {
            nodes: nodes.into_iter().collect(),
            edges: edges.into_iter().collect(),
        };
        elements.sort_dedup();
        elements
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.edges.is_empty()
    }

    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    pub fn edges(&self) -> &[EdgeId] {
        &self.edges
    }

    pub fn into_parts(self) -> (Vec<NodeId>, Vec<EdgeId>) {
        (self.nodes, self.edges)
    }

    fn sort_dedup(&mut self) {
        self.nodes.sort_unstable();
        self.nodes.dedup();
        self.edges.sort_unstable();
        self.edges.dedup();
    }
}

/// Delete request an adapter can pass to an async `onBeforeDelete`-style hook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreDeleteRequest {
    pub requested: DeleteElements,
    pub planned: DeleteElements,
}

impl PreDeleteRequest {
    pub fn new(requested: DeleteElements, planned: DeleteElements) -> Self {
        Self { requested, planned }
    }

    pub fn requested(&self) -> &DeleteElements {
        &self.requested
    }

    pub fn planned(&self) -> &DeleteElements {
        &self.planned
    }
}

/// Adapter decision after an async `onBeforeDelete`-style hook resolves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum PreDeleteResolution {
    /// Commit the preflight's planned delete set.
    Accept,
    /// Cancel deletion.
    Veto,
    /// Commit a replacement delete set after normal policy validation.
    Replace { elements: DeleteElements },
}

impl PreDeleteResolution {
    pub fn accept() -> Self {
        Self::Accept
    }

    pub fn veto() -> Self {
        Self::Veto
    }

    pub fn replace(
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
    ) -> Self {
        Self::Replace {
            elements: DeleteElements::new(nodes, edges),
        }
    }
}

/// Error returned when a delete-selection request could not be committed.
#[derive(Debug, thiserror::Error)]
pub enum DeleteSelectionError {
    /// Rules rejected the selected elements.
    #[error("delete selection was rejected")]
    Rejected {
        /// Diagnostics produced by the delete rules.
        diagnostics: Vec<Diagnostic>,
    },
    /// Store dispatch failed after rules accepted the delete plan.
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
}

impl DeleteSelectionError {
    /// Returns rule diagnostics when the request was rejected by delete policy.
    pub fn diagnostics(&self) -> Option<&[Diagnostic]> {
        match self {
            Self::Rejected { diagnostics } => Some(diagnostics),
            Self::Dispatch(_) => None,
        }
    }
}
