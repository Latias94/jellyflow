//! Revisioned graph commands accepted by the authoritative runtime.

mod model;
mod planning;

pub(crate) use model::GraphCommandExecutionKind;
pub use model::{
    ConnectionDecision, ConnectionEdit, ConnectionEvaluation, GraphCommand, GraphCommandConflict,
    GraphCommandNoop, GraphCommandOutcome, GraphCommandReceipt, GraphCommandRejection,
    IncidentEdgePolicy, NodeGeometryEdit, SymbolEdit, TopologyEdit, TopologyPort,
};
pub(crate) use planning::{GraphCommandPlanningError, PlannedGraphCommand, plan_graph_command};
