//! View-scoped frame, geometry publication, query, and lifecycle capabilities.

pub use crate::runtime::adapter_protocol::{
    AdapterEnvironment, AdapterFrame, DEFAULT_FRAME_REQUIRED_NODE_BUDGET,
    DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET, DEFAULT_FRAME_VISIBLE_NODE_BUDGET,
    DEFAULT_FRAME_VISIBLE_NODE_SCAN_LIMIT, FeedbackCollector, FeedbackCollectorError,
    FeedbackOperationReceipt, FeedbackReceipt, FeedbackSubmissionError, FrameBoundsSource,
    FrameFeedback, FrameMaterializationBudget, FrameMaterializationError,
    FrameMaterializationOverflow, FrameNode, FramePort, FrameRequest, GeometryElementIter,
    GeometryElementKeys, GeometryElementValues, GeometryElements, GeometrySnapshot,
    LayoutNodeStatus, LayoutPublicationReceipt, NodeInstanceKey, NodeLayout, NodeLayoutBuilder,
    NodeLayoutBuilderError, NodeLayoutTarget, PortLayoutOutcome, PortLayoutTarget,
    PublishedEdgeRoute, PublishedNodeGeometry, PublishedPortGeometry, PublishedPortOutcome,
    RenderedPortLayout, RendererStamp, StaleFrameReason,
};
pub use crate::runtime::view_session::{
    AttachViewError, ConnectionProbeCursor, ConnectionProbeCursorStaleReason, ConnectionProbeQuery,
    ConnectionProbeQueryError, ConnectionProbeQueryOutcome, ConnectionProbeTargeting,
    ConnectionProbeWork, EdgePick, EdgePickCursor, EdgePickCursorStaleReason,
    EdgePickGeometryFailure, EdgePickQuery, EdgePickQueryError, EdgePickQueryOutcome, EdgePickWork,
    NodeBindingEvent, NodeBindingKey, NodeIntersectionCursor, NodeIntersectionCursorStaleReason,
    NodeIntersectionQuery, NodeIntersectionQueryError, NodeIntersectionQueryOutcome,
    NodeIntersectionWork, RouteGeometryRevision, ViewCloseOutcome, ViewCloseReason,
    ViewCloseReceipt, ViewCloseRequest, ViewCreateFromWireQueryError, ViewGeneration, ViewId,
    ViewProvenance, ViewSession, ViewSessionError, ViewSessionEvent, ViewSessionOptions,
    ViewSessionStats, ViewSubscriptionToken, VisibleRoute, VisibleRouteCursor,
    VisibleRouteCursorStaleReason, VisibleRouteGeometryOverflow, VisibleRouteQuery,
    VisibleRouteQueryError, VisibleRouteQueryOutcome, VisibleRouteQueryRevision,
    VisibleRouteQueryWork, VisibleRoutes, WeakViewSession,
};
