use super::FitViewNodeInfo;

pub(super) fn project_nodes_to_top_left(
    nodes: &[FitViewNodeInfo],
    node_origin: (f32, f32),
    zoom: f32,
) -> Vec<FitViewNodeInfo> {
    let mut projected = Vec::with_capacity(nodes.len());

    for node in nodes {
        let Some(pos) = node.top_left_at_zoom(node_origin, zoom) else {
            continue;
        };

        projected.push(FitViewNodeInfo {
            pos,
            size_px: node.size_px,
        });
    }

    projected
}
