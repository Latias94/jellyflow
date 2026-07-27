//! Revisioned graph commands accepted by the authoritative runtime.

mod model;
mod planning;

pub use crate::rules::ConnectDecision;
pub(crate) use model::GraphCommandExecutionKind;
pub use model::{
    ConnectionEdit, ConnectionEvaluation, GraphCommand, GraphCommandConflict, GraphCommandIdentity,
    GraphCommandNoop, GraphCommandOutcome, GraphCommandReceipt, GraphCommandRejection,
    IncidentEdgePolicy, NodeGeometryEdit, SymbolEdit, TopologyEdit, TopologyPort,
};
pub(crate) use planning::{GraphCommandPlanningError, PlannedGraphCommand, plan_graph_command};
