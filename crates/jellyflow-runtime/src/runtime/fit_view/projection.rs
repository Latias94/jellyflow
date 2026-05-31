use jellyflow_core::core::CanvasPoint;

use super::FitViewNodeInfo;

pub(super) fn project_nodes_to_top_left(
    nodes: &[FitViewNodeInfo],
    node_origin: (f32, f32),
    zoom: f32,
) -> Vec<FitViewNodeInfo> {
    let mut projected = Vec::with_capacity(nodes.len());
    let (origin_x, origin_y) = node_origin;

    for node in nodes {
        let Some((width_canvas, height_canvas)) = node_canvas_size(node, zoom) else {
            continue;
        };

        projected.push(FitViewNodeInfo {
            pos: CanvasPoint {
                x: node.pos.x - origin_x * width_canvas,
                y: node.pos.y - origin_y * height_canvas,
            },
            size_px: node.size_px,
        });
    }

    projected
}

fn node_canvas_size(node: &FitViewNodeInfo, zoom: f32) -> Option<(f32, f32)> {
    let (width_px, height_px) = node.size_px;
    if !width_px.is_finite() || !height_px.is_finite() || width_px <= 0.0 || height_px <= 0.0 {
        return None;
    }

    Some((width_px / zoom, height_px / zoom))
}
