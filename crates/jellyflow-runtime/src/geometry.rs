//! Renderer-neutral coordinates, edge paths, routing, and hit testing.

pub use crate::runtime::adapter_protocol::{ViewDelta, ViewPoint, ViewRect, ViewSize, side_anchor};
pub use crate::runtime::geometry::{
    BezierEdgeOptions, DEFAULT_EDGE_FLATTEN_ERROR, DEFAULT_EDGE_FLATTEN_MAX_DEPTH,
    DEFAULT_EDGE_FLATTEN_MAX_SEGMENTS, DEFAULT_EDGE_INTERACTION_WIDTH, EdgeEndpointInput,
    EdgeEndpointPosition, EdgeFlattenOptions, EdgeHitTestOptions, EdgeInteractionFacts, EdgePath,
    EdgePathAnchor, EdgePathFlattenError, EdgePathLabel, EdgePathSample, EdgePathSampleError,
    EdgePathSegment, EdgePosition, EdgePresentationFacts, EdgeRouteFacts, FlattenedEdgePath,
    HandleBounds, HandlePosition, OrthogonalEdgeOptions, PathCommand, SmoothStepEdgeOptions,
    bezier_edge_path, edge_path_contains_point, edge_path_distance, edge_position,
    flatten_edge_path, handle_anchor_position, handle_center_position, orthogonal_edge_path,
    resolve_edge_route_path, smoothstep_edge_path, straight_edge_path,
};
