//! Renderer-neutral node resizing helpers.
//!
//! These helpers turn canvas-space resize intent into normal graph transactions without depending
//! on resize handles, pointer capture, DOM state, windowing, or renderer APIs.

mod planner;
mod store;
mod types;

pub use planner::{plan_node_resize, plan_node_resize_with_context};
pub use types::{
    NODE_RESIZE_TRANSACTION_LABEL, NodeResizeConstraints, NodeResizeContext, NodeResizeDirection,
    NodeResizeItem, NodeResizePlan, NodeResizeRequest,
};
