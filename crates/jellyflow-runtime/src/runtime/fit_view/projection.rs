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
        let Some(size_canvas) = node.canvas_size_at_zoom(zoom) else {
            continue;
        };

        projected.push(FitViewNodeInfo {
            pos: CanvasPoint {
                x: node.pos.x - origin_x * size_canvas.width,
                y: node.pos.y - origin_y * size_canvas.height,
            },
            size_px: node.size_px,
        });
    }

    projected
}
