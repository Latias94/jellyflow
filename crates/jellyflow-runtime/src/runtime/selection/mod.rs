//! Renderer-neutral selection helpers.
//!
//! These helpers turn canvas-space marquee rectangles into ordered selection state without
//! depending on a renderer, DOM measurement, or platform input events.

mod activation;
mod additive;
mod compute;
mod edges;
mod node_drag_start;
mod pointer_claim;
mod store;
mod types;

pub use activation::{SelectionDragActivationInput, selection_drag_threshold_met};
pub use compute::{compute_selection_box, resolve_selection_box};
pub use node_drag_start::{
    NodeDragStartSelectionAction, NodeDragStartSelectionInput, NodePointerDownDecision,
    NodePointerDownInput, resolve_node_drag_start_selection, resolve_node_pointer_down,
};
pub use pointer_claim::{
    SelectionPointerClaim, SelectionPointerClaimInput, resolve_selection_pointer_claim,
};
pub use types::{
    SelectionBoxDecision, SelectionBoxInput, SelectionBoxOptions, SelectionBoxResult,
    SelectionModifier,
};
