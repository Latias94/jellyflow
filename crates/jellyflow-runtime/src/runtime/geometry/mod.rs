//! Renderer-neutral geometry primitives for Jellyflow runtimes and adapters.

mod bounds;
mod edge_route;
mod endpoints;
mod hit_test;
mod paths;
mod viewport;

pub(crate) use bounds::CanvasBounds;
pub use edge_route::{
    EdgeInteractionFacts, EdgeRouteFacts, ResolvedEdgeRouteKind, resolve_edge_route_path,
};
pub use endpoints::{
    EdgeEndpointInput, EdgeEndpointPosition, EdgePosition, HandleBounds, HandlePosition,
    edge_position, handle_anchor_position, handle_center_position,
};
pub use hit_test::{EdgeHitTestOptions, edge_path_contains_point, edge_path_distance};
pub use paths::{
    BezierEdgeOptions, EdgePath, EdgePathLabel, PathCommand, SmoothStepEdgeOptions,
    bezier_edge_path, smoothstep_edge_path, straight_edge_path,
};
pub(crate) use viewport::ViewportFitFrame;
