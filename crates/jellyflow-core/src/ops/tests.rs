mod apply;
mod diff;
mod fixtures;
mod fragment;
mod history;
mod mutation;
mod normalize;
mod setters;

use fixtures::{insert_connected_pair, insert_node, insert_port, make_edge, make_node, make_port};

pub(super) use super::{apply::apply_transaction, diff::graph_diff, history::invert_transaction};
pub(super) use crate::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphId, GraphImport,
    Group, GroupId, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey,
    PortKind, SUBGRAPH_NODE_KIND, SYMBOL_REF_NODE_KIND, StickyNote, StickyNoteId, Symbol, SymbolId,
    subgraph_target_graph_id, symbol_ref_target_symbol_id,
};
pub(super) use crate::ops::{
    ApplyError, EdgeEndpoints, GraphFragment, GraphHistory, GraphMutationBatchPlanner,
    GraphMutationError, GraphMutationPlanner, GraphOp, GraphOpBuilderExt, GraphTransaction,
    IdRemapSeed, IdRemapper, PasteTuning, PortInsert,
};
pub(super) use crate::types::TypeDesc;
pub(super) use uuid::Uuid;
