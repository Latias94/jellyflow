use crate::core::{
    BindingId, EdgeId, GraphId, GraphValidationError, GroupId, NodeId, PortId, StickyNoteId,
    SymbolId,
};

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("node already exists: {id:?}")]
    NodeAlreadyExists { id: NodeId },
    #[error("missing node: {id:?}")]
    MissingNode { id: NodeId },
    #[error("port already exists: {id:?}")]
    PortAlreadyExists { id: PortId },
    #[error("missing port: {id:?}")]
    MissingPort { id: PortId },
    #[error("edge already exists: {id:?}")]
    EdgeAlreadyExists { id: EdgeId },
    #[error("missing edge: {id:?}")]
    MissingEdge { id: EdgeId },
    #[error("symbol already exists: {id:?}")]
    SymbolAlreadyExists { id: SymbolId },
    #[error("missing symbol: {id:?}")]
    MissingSymbol { id: SymbolId },
    #[error("group already exists: {id:?}")]
    GroupAlreadyExists { id: GroupId },
    #[error("missing group: {id:?}")]
    MissingGroup { id: GroupId },
    #[error("node parent references missing group: node={node:?} group={group:?}")]
    NodeParentMissingGroup { node: NodeId, group: GroupId },
    #[error("sticky note already exists: {id:?}")]
    StickyNoteAlreadyExists { id: StickyNoteId },
    #[error("missing sticky note: {id:?}")]
    MissingStickyNote { id: StickyNoteId },
    #[error("binding already exists: {id:?}")]
    BindingAlreadyExists { id: BindingId },
    #[error("missing binding: {id:?}")]
    MissingBinding { id: BindingId },
    #[error("node ports list contains unknown port: node={node:?} port={port:?}")]
    NodePortsUnknownPort { node: NodeId, port: PortId },
    #[error("edge references missing port: edge={edge:?} port={port:?}")]
    EdgeMissingPort { edge: EdgeId, port: PortId },
    #[error("import already exists: {id}")]
    ImportAlreadyExists { id: GraphId },
    #[error("missing import: {id}")]
    MissingImport { id: GraphId },
    #[error("remove node op did not match current node: {id:?}")]
    RemoveNodeMismatch { id: NodeId },
    #[error("remove port op did not match current port: {id:?}")]
    RemovePortMismatch { id: PortId },
    #[error("remove edge op did not match current edge: {id:?}")]
    RemoveEdgeMismatch { id: EdgeId },
    #[error("remove symbol op did not match current symbol: {id:?}")]
    RemoveSymbolMismatch { id: SymbolId },
    #[error("remove group op did not match current group: {id:?}")]
    RemoveGroupMismatch { id: GroupId },
    #[error(
        "remove group op expected node parent mismatch: group={group:?} node={node:?} expected={expected:?}"
    )]
    RemoveGroupDetachedMismatch {
        group: GroupId,
        node: NodeId,
        expected: Option<GroupId>,
    },
    #[error("remove sticky note op did not match current note: {id:?}")]
    RemoveStickyNoteMismatch { id: StickyNoteId },
    #[error("remove binding op did not match current binding: {id:?}")]
    RemoveBindingMismatch { id: BindingId },
    #[error("transaction result violates graph invariants: {errors:?}")]
    InvalidTransactionResult { errors: Vec<GraphValidationError> },
}
