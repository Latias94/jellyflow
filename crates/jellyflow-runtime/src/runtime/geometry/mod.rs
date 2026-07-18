//! Renderer-neutral geometry primitives for Jellyflow runtimes and adapters.

mod bounds;
mod edge_route;
mod endpoints;
mod hit_test;
mod paths;

pub(crate) use bounds::CanvasBounds;
pub use edge_route::{
    EdgeInteractionFacts, EdgePresentationFacts, EdgeRouteFacts, ResolvedEdgeRouteKind,
    resolve_edge_route_path,
};
pub use endpoints::{
    EdgeEndpointInput, EdgeEndpointPosition, EdgePosition, HandleBounds, HandlePosition,
    edge_position, handle_anchor_position, handle_center_position,
};
pub(crate) use hit_test::edge_path_conservative_bounds;
pub use hit_test::{
    DEFAULT_EDGE_FLATTEN_ERROR, DEFAULT_EDGE_FLATTEN_MAX_DEPTH, DEFAULT_EDGE_FLATTEN_MAX_SEGMENTS,
    DEFAULT_EDGE_INTERACTION_WIDTH, EdgeFlattenOptions, EdgeHitTestOptions, EdgePathFlattenError,
    EdgePathSegment, FlattenedEdgePath, edge_path_contains_point, edge_path_distance,
    flatten_edge_path,
};
pub use paths::{
    BezierEdgeOptions, EdgePath, EdgePathLabel, OrthogonalEdgeOptions, PathCommand,
    SmoothStepEdgeOptions, bezier_edge_path, orthogonal_edge_path, smoothstep_edge_path,
    straight_edge_path,
};
