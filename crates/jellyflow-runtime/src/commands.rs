//! Revision-bound commands accepted by an authoritative graph runtime.

pub use crate::runtime::commands::{
    CommandCompletionCorrelationError, CommandExecutionCompletion, CommandExecutionCompletions,
    CommandExecutionKind, ConnectDecision, ConnectionEdit, ConnectionEvaluation, GraphCommand,
    GraphCommandConflict, GraphCommandIdentity, GraphCommandNoop, GraphCommandOutcome,
    GraphCommandReceipt, GraphCommandRejection, IncidentEdgePolicy, NodeGeometryEdit, SymbolEdit,
    TopologyEdit, TopologyPort,
};
