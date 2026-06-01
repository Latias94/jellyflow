use super::super::endpoints::EdgeEndpointPosition;
use super::label::edge_center_label;
use super::types::{EdgePath, PathCommand};

pub fn straight_edge_path(
    source: EdgeEndpointPosition,
    target: EdgeEndpointPosition,
) -> Option<EdgePath> {
    let label = edge_center_label(source.point, target.point)?;
    Some(EdgePath {
        commands: vec![
            PathCommand::MoveTo(source.point),
            PathCommand::LineTo(target.point),
        ],
        label,
    })
}
