use crate::runtime::rendering::order::{
    EdgeRenderOrderOptions, GroupRenderOrderOptions, NodeRenderOrderOptions,
};
use crate::runtime::rendering::query::{RenderingQueryOptions, RenderingQueryResult};
use crate::runtime::rendering::visibility::{VisibleEdgeIdsRequest, VisibleNodeIdsRequest};
use crate::runtime::viewport::ViewportTransform;
use jellyflow_core::core::CanvasSize;

use super::backend::NodeGraphQuerySnapshot;

pub(crate) fn resolve_rendering_read_model(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    viewport_size: CanvasSize,
) -> RenderingQueryResult {
    crate::runtime::rendering::query::resolve_rendering_query(
        snapshot.graph,
        snapshot.lookups,
        snapshot.view_state,
        rendering_query_options(snapshot, viewport_size),
    )
}

fn rendering_query_options(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    viewport_size: CanvasSize,
) -> RenderingQueryOptions {
    RenderingQueryOptions::new(
        GroupRenderOrderOptions::from_interaction(&snapshot.interaction),
        NodeRenderOrderOptions::from_interaction(&snapshot.interaction),
        EdgeRenderOrderOptions::from_interaction(&snapshot.interaction),
    )
    .with_visible_nodes(visible_node_ids_request(snapshot, viewport_size))
    .with_visible_edges(visible_edge_ids_request(snapshot, viewport_size))
}

fn visible_node_ids_request(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    viewport_size: CanvasSize,
) -> Option<VisibleNodeIdsRequest> {
    let transform = ViewportTransform::from_view_state(snapshot.view_state)?;
    let rendering = snapshot.interaction.rendering_interaction();
    Some(
        VisibleNodeIdsRequest::new(transform, viewport_size)
            .with_only_render_visible_elements(rendering.only_render_visible_elements)
            .with_node_origin(snapshot.node_origin()),
    )
}

fn visible_edge_ids_request(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    viewport_size: CanvasSize,
) -> Option<VisibleEdgeIdsRequest> {
    let transform = ViewportTransform::from_view_state(snapshot.view_state)?;
    let rendering = snapshot.interaction.rendering_interaction();
    Some(
        VisibleEdgeIdsRequest::new(transform, viewport_size)
            .with_only_render_visible_elements(rendering.only_render_visible_elements)
            .with_node_origin(snapshot.node_origin()),
    )
}
