//! Renderer-neutral ordering and visibility helpers.
//!
//! Adapters still own painting, widgets, and GPU/UI details. This module only resolves the stable
//! order and viewport-visible ids they should use when interpreting Jellyflow view-state.

mod order;
mod query;
mod store;
mod visibility;

pub use order::{
    EdgeRenderOrderOptions, GroupRenderOrderOptions, NodeRenderOrderOptions,
    resolve_edge_render_order, resolve_group_render_order, resolve_node_render_order,
};
pub use query::{RenderingQueryOptions, RenderingQueryResult, resolve_rendering_query};
pub use visibility::{
    VisibleEdgeIdsRequest, VisibleNodeIdsRequest, resolve_visible_edge_ids,
    resolve_visible_edge_render_order, resolve_visible_node_ids, resolve_visible_node_render_order,
};
