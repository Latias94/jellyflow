use jellyflow_core::core::{BindingId, EdgeId, GroupId, NodeId, PortId};

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub(crate) enum GraphMutationError {
    #[error("node already exists: {0:?}")]
    NodeAlreadyExists(NodeId),
    #[error("port already exists: {0:?}")]
    PortAlreadyExists(PortId),
    #[error("missing port: {0:?}")]
    MissingPort(PortId),
    #[error("missing edge: {0:?}")]
    MissingEdge(EdgeId),
    #[error("missing group: {0:?}")]
    MissingGroup(GroupId),
    #[error("missing binding: {0:?}")]
    MissingBinding(BindingId),
}
