//! View-scoped frame, geometry publication, query, and lifecycle capabilities.

pub use crate::runtime::adapter_protocol::{
    AdapterEnvironment, AdapterFrame, DEFAULT_FRAME_PORT_SCAN_LIMIT,
    DEFAULT_FRAME_REQUIRED_NODE_BUDGET, DEFAULT_FRAME_REQUIRED_NODE_SCAN_LIMIT,
    DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET, DEFAULT_FRAME_ROUTING_DEPENDENCY_SCAN_LIMIT,
    DEFAULT_FRAME_VISIBLE_NODE_BUDGET, DEFAULT_FRAME_VISIBLE_NODE_SCAN_LIMIT, FeedbackCollector,
    FeedbackCollectorError, FeedbackLayoutWork, FeedbackReceipt, FeedbackSubmissionError,
    FeedbackWorkReceipt, FrameAdmissionReceipt, FrameBoundsSource, FrameFeedback,
    FrameGeometryReceipt, FrameIncidentRoutingReceipt, FrameIncidentSnapshotReceipt,
    FrameInputReceipt, FrameMaterializationBudget, FrameMaterializationError,
    FrameMaterializationOverflow, FrameMaterializationReceipt, FrameNode, FramePort, FrameRequest,
    GeometryElementIter, GeometryElementKeys, GeometryElementValues, GeometryElements,
    GeometryImpactMergeWork, GeometrySnapshot, LayoutNodeStatus, LayoutOperationReceipt,
    LayoutPublicationReceipt, NodeInstanceKey, NodeLayout, NodeLayoutBuilder,
    NodeLayoutBuilderError, NodeLayoutTarget, PortLayoutOutcome, PortLayoutTarget,
    PublishedEdgeRoute, PublishedNodeGeometry, PublishedPortGeometry, PublishedPortOutcome,
    RenderedPortLayout, RendererStamp, StaleFrameReason,
};
pub use crate::runtime::view_session::{
    AttachViewError, ConnectionProbeQueryReceipt, ConnectionProbeWork, EdgePick, EdgePickCursor,
    EdgePickCursorStaleReason, EdgePickGeometryFailure, EdgePickQuery, EdgePickQueryError,
    EdgePickQueryOutcome, EdgePickQueryReceipt, EdgePickWork, NodeBindingEvent, NodeBindingKey,
    NodeIntersectionCursor, NodeIntersectionCursorStaleReason, NodeIntersectionQuery,
    NodeIntersectionQueryError, NodeIntersectionQueryOutcome, NodeIntersectionQueryReceipt,
    NodeIntersectionWork, RouteGeometryRevision, RuntimePatchFactReceipt,
    RuntimePatchGeometryReceipt, RuntimePatchReconciliationReceipt, RuntimePatchSelectionReceipt,
    ViewCloseOutcome, ViewCloseReason, ViewCloseRequest, ViewCreateFromWireQueryError,
    ViewGeneration, ViewId, ViewProvenance, ViewSession, ViewSessionError, ViewSessionEvent,
    ViewSessionOptions, ViewSubscriptionToken, VisibleRoute, VisibleRouteCursor,
    VisibleRouteCursorStaleReason, VisibleRouteGeometryOverflow, VisibleRouteQuery,
    VisibleRouteQueryError, VisibleRouteQueryOutcome, VisibleRouteQueryRevision,
    VisibleRouteQueryWork, VisibleRoutes, WeakViewSession,
};
