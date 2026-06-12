//! Adapter-facing helpers for schema-driven node creation.

use serde::{Deserialize, Serialize};

use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};
use crate::schema::{NodeInstantiation, NodeInstantiationError, NodeRegistry};
use jellyflow_core::core::{CanvasPoint, NodeId, NodeKindKey, PortId};

/// Default transaction label for schema-driven create-node commits.
pub const CREATE_NODE_TRANSACTION_LABEL: &str = "create node";

/// Canvas-space request for creating one node from a registered schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateNodeRequest {
    /// Node kind or alias selected by the adapter palette.
    pub kind: NodeKindKey,
    /// Top-left position in canvas space.
    pub pos: CanvasPoint,
}

impl CreateNodeRequest {
    pub fn new(kind: NodeKindKey, pos: CanvasPoint) -> Self {
        Self { kind, pos }
    }
}

/// Result of a store create-node commit.
#[derive(Debug, Clone)]
pub struct CreateNodeOutcome {
    /// Instantiated graph records before profile-derived edits are applied.
    pub instantiation: NodeInstantiation,
    /// Store dispatch result, including the committed transaction patch.
    pub dispatch: DispatchOutcome,
}

impl CreateNodeOutcome {
    pub fn node_id(&self) -> NodeId {
        self.instantiation.node_id
    }

    pub fn port_ids(&self) -> impl Iterator<Item = PortId> + '_ {
        self.instantiation.ports.iter().map(|(id, _)| *id)
    }
}

/// Error returned when schema-driven node creation cannot be committed.
#[derive(Debug, thiserror::Error)]
pub enum CreateNodeError {
    #[error(transparent)]
    Schema(#[from] NodeInstantiationError),
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
}

impl NodeGraphStore {
    /// Instantiates a schema node without mutating the store.
    pub fn plan_create_node_from_schema(
        &self,
        registry: &NodeRegistry,
        request: CreateNodeRequest,
    ) -> Result<NodeInstantiation, NodeInstantiationError> {
        registry.instantiate_node(&request.kind, request.pos)
    }

    /// Instantiates and commits a schema node through the normal store dispatch path.
    pub fn apply_create_node_from_schema(
        &mut self,
        registry: &NodeRegistry,
        request: CreateNodeRequest,
    ) -> Result<CreateNodeOutcome, CreateNodeError> {
        let instantiation = self.plan_create_node_from_schema(registry, request)?;
        let tx = instantiation
            .clone()
            .into_labeled_transaction(CREATE_NODE_TRANSACTION_LABEL);
        let dispatch = self.dispatch_transaction(&tx)?;

        Ok(CreateNodeOutcome {
            instantiation,
            dispatch,
        })
    }
}
