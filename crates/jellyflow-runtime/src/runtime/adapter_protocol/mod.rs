//! Framework-neutral frame materialization and native layout feedback protocol.
//!
//! Native hosts report framework view-space logical pixels. The protocol converts them into
//! canvas-space geometry internally. UI framework entities, widget trees, callbacks, and borrowed
//! layout objects stay inside each adapter.

mod coordinates;
mod feedback;
mod frame;
mod geometry;
mod work;

#[cfg(feature = "adapter-conformance")]
pub mod conformance;

pub use coordinates::{ViewDelta, ViewPoint, ViewRect, ViewSize, side_anchor};
pub use feedback::{
    FeedbackCollector, FeedbackCollectorError, FeedbackLayoutWork, FeedbackReceipt,
    FeedbackSubmissionError, FeedbackWorkReceipt, FrameFeedback, FrameMaterializationError,
    LayoutNodeStatus, LayoutOperationReceipt, LayoutPublicationReceipt, NodeLayout,
    NodeLayoutBuilder, NodeLayoutBuilderError, PortLayoutOutcome, RenderedPortLayout,
    StaleFrameReason,
};
pub use frame::{
    AdapterEnvironment, AdapterFrame, DEFAULT_FRAME_PORT_SCAN_LIMIT,
    DEFAULT_FRAME_REQUIRED_NODE_BUDGET, DEFAULT_FRAME_REQUIRED_NODE_SCAN_LIMIT,
    DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET, DEFAULT_FRAME_ROUTING_DEPENDENCY_SCAN_LIMIT,
    DEFAULT_FRAME_VISIBLE_NODE_BUDGET, DEFAULT_FRAME_VISIBLE_NODE_SCAN_LIMIT,
    FrameAdmissionReceipt, FrameBoundsSource, FrameGeometryReceipt, FrameIncidentRoutingReceipt,
    FrameIncidentSnapshotReceipt, FrameInputReceipt, FrameMaterializationBudget,
    FrameMaterializationOverflow, FrameMaterializationReceipt, FrameNode, FramePort, FrameRequest,
    NodeInstanceKey, NodeLayoutTarget, PortLayoutTarget, RendererStamp,
};
pub use geometry::{
    GeometryElementIter, GeometryElementKeys, GeometryElementValues, GeometryElements,
    GeometrySnapshot, PublishedEdgeRoute, PublishedNodeGeometry, PublishedPortGeometry,
    PublishedPortOutcome,
};
pub(crate) use geometry::{
    GeometryVisibleCounts, SharedFrameCullSet, SharedGeometryMap, SharedGeometryOwnerMap,
};
pub use work::GeometryImpactMergeWork;

pub(crate) use feedback::FeedbackOperation;
pub(crate) use frame::FrameToken;
