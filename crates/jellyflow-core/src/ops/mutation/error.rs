use crate::core::{BindingId, EdgeId, GroupId, NodeId, PortId, StickyNoteId};

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum GraphMutationError {
    #[error("node already exists: {0:?}")]
    NodeAlreadyExists(NodeId),
    #[error("missing node: {0:?}")]
    MissingNode(NodeId),
    #[error("port already exists: {0:?}")]
    PortAlreadyExists(PortId),
    #[error("missing port: {0:?}")]
    MissingPort(PortId),
    #[error("edge already exists: {0:?}")]
    EdgeAlreadyExists(EdgeId),
    #[error("missing edge: {0:?}")]
    MissingEdge(EdgeId),
    #[error("missing group: {0:?}")]
    MissingGroup(GroupId),
    #[error("binding already exists: {0:?}")]
    BindingAlreadyExists(BindingId),
    #[error("missing binding: {0:?}")]
    MissingBinding(BindingId),
    #[error("missing sticky note: {0:?}")]
    MissingStickyNote(StickyNoteId),
    #[error("port owner mismatch: port={port:?} expected={expected:?} got={got:?}")]
    PortOwnerMismatch {
        port: PortId,
        expected: NodeId,
        got: NodeId,
    },
    #[error("duplicate port in node planning: node={node:?} port={port:?}")]
    DuplicateNodePort { node: NodeId, port: PortId },
    #[error("port insert index out of bounds: node={node:?} index={index} len={len}")]
    PortInsertOutOfBounds {
        node: NodeId,
        index: usize,
        len: usize,
    },
}
