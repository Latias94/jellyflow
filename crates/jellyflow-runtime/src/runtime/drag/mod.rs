//! Renderer-neutral node dragging helpers.
//!
//! These helpers turn canvas-space drag intent into normal graph transactions without depending on
//! pointer capture, DOM state, windowing, or renderer APIs.

mod activation;
mod candidates;
mod constraints;
mod planner;
mod store;
mod types;

pub use activation::{NodeDragActivationInput, node_drag_threshold_met};
pub use planner::{plan_node_drag, plan_node_nudge};
pub use types::{
    NODE_DRAG_TRANSACTION_LABEL, NODE_NUDGE_TRANSACTION_LABEL, NodeDragItem, NodeDragPlan,
    NodeDragRequest, NodeNudgeDirection, NodeNudgePlan, NodeNudgeRequest,
};
