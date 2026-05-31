use crate::core::{
    EdgeId, EdgeKind, GraphId, GroupId, NodeId, PortCapacity, PortId, PortKind, SubgraphNodeError,
    SymbolId, SymbolRefNodeError,
};

#[derive(Debug, thiserror::Error)]
pub enum GraphValidationError {
    #[error("graph version mismatch: expected={expected} found={found}")]
    UnsupportedGraphVersion { expected: u32, found: u32 },

    #[error("port references missing node: port={port:?} node={node:?}")]
    PortMissingNode { port: PortId, node: NodeId },

    #[error("port is missing from owner node ports list: port={port:?} node={node:?}")]
    PortMissingFromOwner { port: PortId, node: NodeId },

    #[error("node parent references missing group: node={node:?} group={group:?}")]
    NodeParentMissingGroup { node: NodeId, group: GroupId },

    #[error("node has invalid size: node={node:?} width={width} height={height}")]
    NodeInvalidSize {
        node: NodeId,
        width: f32,
        height: f32,
    },

    #[error("node ports list references missing port: node={node:?} port={port:?}")]
    NodePortsMissingPort { node: NodeId, port: PortId },

    #[error(
        "node ports list references port owned by another node: node={node:?} port={port:?} owner={owner:?}"
    )]
    NodePortsWrongOwner {
        node: NodeId,
        port: PortId,
        owner: NodeId,
    },

    #[error("node ports list contains duplicates: node={node:?} port={port:?}")]
    NodePortsDuplicate { node: NodeId, port: PortId },

    #[error("edge references missing port: edge={edge:?} port={port:?}")]
    EdgeMissingPort { edge: EdgeId, port: PortId },

    #[error(
        "edge port kinds are incompatible: edge={edge:?} from_kind={from_kind:?} to_kind={to_kind:?}"
    )]
    EdgeKindMismatch {
        edge: EdgeId,
        from_kind: PortKind,
        to_kind: PortKind,
    },

    #[error(
        "edge kind does not match port kind: edge={edge:?} edge_kind={edge_kind:?} port_kind={port_kind:?}"
    )]
    EdgeKindPortKindMismatch {
        edge: EdgeId,
        edge_kind: EdgeKind,
        port_kind: PortKind,
    },

    #[error("edge duplicates an existing connection: edge={edge:?}")]
    DuplicateEdge { edge: EdgeId },

    #[error("port capacity exceeded: port={port:?} capacity={capacity:?} count={count}")]
    PortCapacityExceeded {
        port: PortId,
        capacity: PortCapacity,
        count: usize,
    },

    #[error("subgraph node missing graph_id: node={node:?}")]
    SubgraphNodeMissingGraphId { node: NodeId },

    #[error("subgraph node graph_id is not a string: node={node:?}")]
    SubgraphNodeGraphIdNotString { node: NodeId },

    #[error("subgraph node graph_id is not a valid uuid: node={node:?} value={value:?}")]
    SubgraphNodeInvalidGraphId { node: NodeId, value: String },

    #[error(
        "subgraph node target graph is not declared in imports: node={node:?} graph_id={graph_id}"
    )]
    SubgraphTargetNotImported { node: NodeId, graph_id: GraphId },

    #[error("symbol ref node missing symbol_id: node={node:?}")]
    SymbolRefNodeMissingSymbolId { node: NodeId },

    #[error("symbol ref node symbol_id is not a string: node={node:?}")]
    SymbolRefNodeSymbolIdNotString { node: NodeId },

    #[error("symbol ref node symbol_id is not a valid uuid: node={node:?} value={value:?}")]
    SymbolRefNodeInvalidSymbolId { node: NodeId, value: String },

    #[error(
        "symbol ref node target symbol is not declared in symbols: node={node:?} symbol_id={symbol_id:?}"
    )]
    SymbolRefTargetNotDeclared { node: NodeId, symbol_id: SymbolId },
}

impl From<SubgraphNodeError> for GraphValidationError {
    fn from(err: SubgraphNodeError) -> Self {
        match err {
            SubgraphNodeError::MissingGraphId { node } => Self::SubgraphNodeMissingGraphId { node },
            SubgraphNodeError::GraphIdNotString { node } => {
                Self::SubgraphNodeGraphIdNotString { node }
            }
            SubgraphNodeError::InvalidGraphId { node, value } => {
                Self::SubgraphNodeInvalidGraphId { node, value }
            }
        }
    }
}

impl From<SymbolRefNodeError> for GraphValidationError {
    fn from(err: SymbolRefNodeError) -> Self {
        match err {
            SymbolRefNodeError::MissingSymbolId { node } => {
                Self::SymbolRefNodeMissingSymbolId { node }
            }
            SymbolRefNodeError::SymbolIdNotString { node } => {
                Self::SymbolRefNodeSymbolIdNotString { node }
            }
            SymbolRefNodeError::InvalidSymbolId { node, value } => {
                Self::SymbolRefNodeInvalidSymbolId { node, value }
            }
        }
    }
}
