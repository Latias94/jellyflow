//! Framework-neutral frame materialization and native layout feedback protocol.
//!
//! Native hosts report framework view-space logical pixels. The protocol converts them into
//! canvas-space geometry internally. UI framework entities, widget trees, callbacks, and borrowed
//! layout objects stay inside each adapter.

mod coordinates;
mod feedback;
mod frame;
mod geometry;

pub use coordinates::{ViewDelta, ViewPoint, ViewRect, ViewSize, side_anchor};
pub use feedback::{
    FeedbackCollector, FeedbackCollectorError, FeedbackOperationReceipt, FeedbackReceipt,
    FeedbackSubmissionError, FrameFeedback, FrameMaterializationError, HandlePosition,
    LayoutNodeStatus, LayoutPublicationReceipt, NodeLayout, NodeLayoutBuilder,
    NodeLayoutBuilderError, PortLayoutOutcome, RenderedPortLayout, StaleFrameReason,
};
pub use frame::{
    AdapterEnvironment, AdapterFrame, DEFAULT_FRAME_REQUIRED_NODE_BUDGET,
    DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET, DEFAULT_FRAME_VISIBLE_NODE_BUDGET,
    DEFAULT_FRAME_VISIBLE_NODE_SCAN_LIMIT, FrameBoundsSource, FrameMaterializationBudget,
    FrameMaterializationOverflow, FrameNode, FramePort, FrameRequest, NodeInstanceKey,
    NodeLayoutTarget, PortLayoutTarget, RendererStamp,
};
pub(crate) use geometry::SharedGeometryMap;
pub use geometry::{
    GeometryElementIter, GeometryElementKeys, GeometryElementValues, GeometryElements,
    GeometrySnapshot, PublishedEdgeRoute, PublishedNodeGeometry, PublishedPortGeometry,
    PublishedPortOutcome,
};

pub(crate) use feedback::FeedbackOperation;
pub(crate) use frame::FrameToken;
