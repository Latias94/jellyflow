//! Renderer-neutral selection helpers.
//!
//! These helpers turn canvas-space marquee rectangles into ordered selection state without
//! depending on a renderer, DOM measurement, or platform input events.

mod additive;
mod compute;
mod edges;
mod store;
mod types;

pub use compute::compute_selection_box;
pub use types::{SelectionBoxOptions, SelectionBoxResult};
