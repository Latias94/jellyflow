//! Renderer-neutral auto-pan helpers.
//!
//! Adapters own frame scheduling and raw pointer capture. The runtime owns deterministic
//! screen-space edge-proximity math and feeds the existing viewport pan path.

mod planner;
mod store;
mod types;

pub use planner::{compute_auto_pan, compute_selection_auto_pan};
pub use types::{
    AutoPanActivation, AutoPanOutcome, AutoPanPlan, AutoPanRequest, SelectionAutoPanRequest,
};
