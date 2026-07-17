use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId, PortId};

use crate::io::ViewSelectionSeed;
use crate::runtime::commands::{GraphCommand, GraphCommandOutcome};
pub use crate::runtime::geometry::HandlePosition;
use crate::runtime::view_session::{ViewId, ViewSessionError};
use crate::runtime::viewport::ViewportTransform;

use super::frame::{FrameToken, NodeLayoutTarget, PortLayoutTarget};
use super::{ViewPoint, ViewRect};

/// Framework view-space geometry for one rendered real graph port.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPortLayout {
    pub(crate) target: PortLayoutTarget,
    pub(crate) visual_bounds: ViewRect,
    pub(crate) hit_bounds: ViewRect,
    pub(crate) route_anchor: ViewPoint,
    pub(crate) outward: HandlePosition,
}

impl RenderedPortLayout {
    fn new(
        target: &PortLayoutTarget,
        visual_bounds: ViewRect,
        hit_bounds: ViewRect,
        route_anchor: ViewPoint,
        outward: HandlePosition,
    ) -> Self {
        Self {
            target: target.clone(),
            visual_bounds,
            hit_bounds,
            route_anchor,
            outward,
        }
    }

    pub fn port(&self) -> PortId {
        self.target.port
    }

    pub fn target(&self) -> &PortLayoutTarget {
        &self.target
    }

    /// Bounds of the native element that visually represents the port.
    pub fn visual_bounds(&self) -> ViewRect {
        self.visual_bounds
    }

    /// Bounds used by the adapter for pointer targeting.
    pub fn hit_bounds(&self) -> ViewRect {
        self.hit_bounds
    }

    /// Exact point where the routed edge meets the port.
    pub fn route_anchor(&self) -> ViewPoint {
        self.route_anchor
    }

    /// Cardinal direction in which the route leaves the anchor.
    pub fn outward(&self) -> HandlePosition {
        self.outward
    }

    #[cfg(test)]
    pub(crate) fn raw_for_test(
        target: &PortLayoutTarget,
        visual_bounds: ViewRect,
        hit_bounds: ViewRect,
        route_anchor: ViewPoint,
        outward: HandlePosition,
    ) -> Self {
        Self::new(target, visual_bounds, hit_bounds, route_anchor, outward)
    }
}

/// Complete layout outcome for one real graph port in a node frame.
#[derive(Debug, Clone, PartialEq)]
pub enum PortLayoutOutcome {
    Rendered(RenderedPortLayout),
    Hidden { target: PortLayoutTarget },
    Culled { target: PortLayoutTarget },
    Unavailable { target: PortLayoutTarget },
}

impl PortLayoutOutcome {
    pub fn port(&self) -> PortId {
        self.target().port()
    }

    pub fn target(&self) -> &PortLayoutTarget {
        match self {
            Self::Rendered(layout) => layout.target(),
            Self::Hidden { target } | Self::Culled { target } | Self::Unavailable { target } => {
                target
            }
        }
    }

    pub fn rendered(&self) -> Option<&RenderedPortLayout> {
        match self {
            Self::Rendered(layout) => Some(layout),
            Self::Hidden { .. } | Self::Culled { .. } | Self::Unavailable { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeLayoutBuilder {
    target: NodeLayoutTarget,
    bounds: ViewRect,
    expected_order: Vec<PortId>,
    expected_targets: BTreeMap<PortId, PortLayoutTarget>,
    outcomes: BTreeMap<PortId, PortLayoutOutcome>,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum NodeLayoutBuilderError {
    #[error("port {port:?} belongs to node {actual_node:?}, not layout node {expected_node:?}")]
    ForeignPort {
        expected_node: NodeId,
        actual_node: NodeId,
        port: PortId,
    },
    #[error("port {port:?} is not part of layout node {node:?}")]
    UnknownPort { node: NodeId, port: PortId },
    #[error("port {port:?} already has a layout outcome")]
    DuplicatePort { port: PortId },
    #[error("layout is missing outcomes for real ports {ports:?}")]
    MissingPorts { ports: Vec<PortId> },
}

impl NodeLayoutBuilder {
    pub(crate) fn new(
        target: &NodeLayoutTarget,
        bounds: ViewRect,
        ports: impl IntoIterator<Item = PortLayoutTarget>,
    ) -> Self {
        let ports = ports.into_iter().collect::<Vec<_>>();
        Self {
            target: target.clone(),
            bounds,
            expected_order: ports.iter().map(PortLayoutTarget::port).collect(),
            expected_targets: ports
                .into_iter()
                .map(|target| (target.port(), target))
                .collect(),
            outcomes: BTreeMap::new(),
        }
    }

    pub fn port_rendered(
        &mut self,
        target: &PortLayoutTarget,
        visual_bounds: ViewRect,
        hit_bounds: ViewRect,
        route_anchor: ViewPoint,
        outward: HandlePosition,
    ) -> Result<&mut Self, NodeLayoutBuilderError> {
        self.record(PortLayoutOutcome::Rendered(RenderedPortLayout::new(
            target,
            visual_bounds,
            hit_bounds,
            route_anchor,
            outward,
        )))
    }

    pub fn port_hidden(
        &mut self,
        target: &PortLayoutTarget,
    ) -> Result<&mut Self, NodeLayoutBuilderError> {
        self.record(PortLayoutOutcome::Hidden {
            target: target.clone(),
        })
    }

    pub fn port_culled(
        &mut self,
        target: &PortLayoutTarget,
    ) -> Result<&mut Self, NodeLayoutBuilderError> {
        self.record(PortLayoutOutcome::Culled {
            target: target.clone(),
        })
    }

    pub fn port_unavailable(
        &mut self,
        target: &PortLayoutTarget,
    ) -> Result<&mut Self, NodeLayoutBuilderError> {
        self.record(PortLayoutOutcome::Unavailable {
            target: target.clone(),
        })
    }

    pub fn finish(self) -> Result<NodeLayout, NodeLayoutBuilderError> {
        let missing = self
            .expected_order
            .iter()
            .filter(|port| !self.outcomes.contains_key(port))
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(NodeLayoutBuilderError::MissingPorts { ports: missing });
        }
        let mut outcomes = self.outcomes;
        Ok(NodeLayout {
            target: self.target,
            bounds: self.bounds,
            port_outcomes: self
                .expected_order
                .into_iter()
                .filter_map(|port| outcomes.remove(&port))
                .collect(),
        })
    }

    fn record(&mut self, outcome: PortLayoutOutcome) -> Result<&mut Self, NodeLayoutBuilderError> {
        let target = outcome.target();
        let port = target.port();
        if target.node.as_ref() != &self.target {
            return Err(NodeLayoutBuilderError::ForeignPort {
                expected_node: self.target.instance().node(),
                actual_node: target.instance().node(),
                port,
            });
        }
        if !self.expected_targets.contains_key(&port) {
            return Err(NodeLayoutBuilderError::UnknownPort {
                node: self.target.instance().node(),
                port,
            });
        }
        if self.outcomes.contains_key(&port) {
            return Err(NodeLayoutBuilderError::DuplicatePort { port });
        }
        self.outcomes.insert(port, outcome);
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeLayout {
    pub(crate) target: NodeLayoutTarget,
    pub(crate) bounds: ViewRect,
    pub(crate) port_outcomes: Vec<PortLayoutOutcome>,
}

impl NodeLayout {
    pub fn node(&self) -> NodeId {
        self.target.instance.node()
    }

    pub fn target(&self) -> &NodeLayoutTarget {
        &self.target
    }

    pub fn bounds(&self) -> ViewRect {
        self.bounds
    }

    pub fn port_outcomes(&self) -> &[PortLayoutOutcome] {
        &self.port_outcomes
    }

    #[cfg(test)]
    pub(crate) fn raw_for_test(
        target: &NodeLayoutTarget,
        bounds: ViewRect,
        port_outcomes: impl IntoIterator<Item = PortLayoutOutcome>,
    ) -> Self {
        Self {
            target: target.clone(),
            bounds,
            port_outcomes: port_outcomes.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum FeedbackOperation {
    Selection {
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    },
    Viewport {
        pan: CanvasPoint,
        zoom: f32,
    },
    Command(GraphCommand),
    Layout(NodeLayout),
}

#[derive(Debug)]
pub struct FrameFeedback {
    pub(crate) token: FrameToken,
    pub(crate) operations: Vec<FeedbackOperation>,
}

#[cfg(test)]
impl FrameFeedback {
    pub(crate) fn duplicate_for_test(&self) -> Self {
        Self {
            token: self.token.clone(),
            operations: self.operations.clone(),
        }
    }

    pub(crate) fn with_sequence_for_test(mut self, sequence: u64) -> Self {
        self.token.sequence = sequence;
        self
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FeedbackCollectorError {
    #[error("feedback collector is already closed")]
    CollectorClosed,
    #[error("feedback collector is busy in another callback")]
    CollectorBusy,
    #[error("viewport pan must be finite and zoom must be positive and finite")]
    InvalidViewport,
}

#[derive(Debug)]
struct FeedbackCollectorState {
    token: Option<FrameToken>,
    operations: Vec<FeedbackOperation>,
}

#[derive(Debug, Clone)]
pub struct FeedbackCollector {
    state: Rc<RefCell<FeedbackCollectorState>>,
}

impl FeedbackCollector {
    pub(crate) fn new(token: FrameToken) -> Self {
        Self {
            state: Rc::new(RefCell::new(FeedbackCollectorState {
                token: Some(token),
                operations: Vec::new(),
            })),
        }
    }

    pub fn selection(
        &self,
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
        groups: impl IntoIterator<Item = GroupId>,
    ) -> Result<(), FeedbackCollectorError> {
        self.push(FeedbackOperation::Selection {
            nodes: nodes.into_iter().collect(),
            edges: edges.into_iter().collect(),
            groups: groups.into_iter().collect(),
        })
    }

    pub fn viewport(&self, pan: CanvasPoint, zoom: f32) -> Result<(), FeedbackCollectorError> {
        let transform =
            ViewportTransform::new(pan, zoom).ok_or(FeedbackCollectorError::InvalidViewport)?;
        self.push(FeedbackOperation::Viewport {
            pan: transform.pan,
            zoom: transform.zoom,
        })
    }

    pub fn command(&self, command: GraphCommand) -> Result<(), FeedbackCollectorError> {
        self.push(FeedbackOperation::Command(command))
    }

    pub fn layout(&self, layout: NodeLayout) -> Result<(), FeedbackCollectorError> {
        self.push(FeedbackOperation::Layout(layout))
    }

    pub fn finish(&self) -> Result<FrameFeedback, FeedbackCollectorError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| FeedbackCollectorError::CollectorBusy)?;
        let token = state
            .token
            .take()
            .ok_or(FeedbackCollectorError::CollectorClosed)?;
        Ok(FrameFeedback {
            token,
            operations: std::mem::take(&mut state.operations),
        })
    }

    fn push(&self, operation: FeedbackOperation) -> Result<(), FeedbackCollectorError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| FeedbackCollectorError::CollectorBusy)?;
        if state.token.is_none() {
            return Err(FeedbackCollectorError::CollectorClosed);
        }
        state.operations.push(operation);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleFrameReason {
    Superseded,
    GraphRevision,
    DocumentEpoch,
    Transform,
    Environment,
    NodeLayoutGeneration,
}

#[derive(Debug, thiserror::Error)]
pub enum FrameMaterializationError {
    #[error(transparent)]
    Session(#[from] ViewSessionError),
    #[error("adapter frame viewport is not positive and finite")]
    InvalidViewport,
    #[error("adapter frame identity allocator is exhausted")]
    IdentityExhausted,
    #[error("adapter frame {sequence} became stale while being materialized: {reason:?}")]
    StaleFrame {
        sequence: u64,
        reason: StaleFrameReason,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum FeedbackSubmissionError {
    #[error(transparent)]
    Session(#[from] ViewSessionError),
    #[error("feedback frame belongs to view {actual:?}, not {expected:?}")]
    ForeignFrame { expected: ViewId, actual: ViewId },
    #[error("feedback frame {sequence} has already been consumed")]
    ReplayedFrame { sequence: u64 },
    #[error("feedback frame {sequence} is newer than the latest issued frame")]
    FutureFrame { sequence: u64 },
    #[error("feedback frame {sequence} is stale: {reason:?}")]
    StaleFrame {
        sequence: u64,
        reason: StaleFrameReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutNodeStatus {
    Published,
    Unchanged,
    Stale,
    MissingNode,
    Hidden,
    Culled,
    InvalidBounds,
    IncompletePorts,
    DuplicatePorts,
    ForeignPort,
    DuplicateNode,
}

#[derive(Debug)]
pub enum FeedbackOperationReceipt {
    Selection {
        selection: ViewSelectionSeed,
    },
    Viewport {
        transform: ViewportTransform,
    },
    Command {
        outcome: Box<Result<GraphCommandOutcome, ViewSessionError>>,
    },
    Layout {
        node: NodeId,
        status: LayoutNodeStatus,
    },
}

impl FeedbackOperationReceipt {
    pub fn layout_status(&self) -> Option<LayoutNodeStatus> {
        match self {
            Self::Layout { status, .. } => Some(*status),
            Self::Selection { .. } | Self::Viewport { .. } | Self::Command { .. } => None,
        }
    }

    pub fn selection(&self) -> Option<&ViewSelectionSeed> {
        match self {
            Self::Selection { selection } => Some(selection),
            Self::Viewport { .. } | Self::Command { .. } | Self::Layout { .. } => None,
        }
    }

    pub fn viewport_transform(&self) -> Option<ViewportTransform> {
        match self {
            Self::Viewport { transform } => Some(*transform),
            Self::Selection { .. } | Self::Command { .. } | Self::Layout { .. } => None,
        }
    }

    pub fn command_outcome(&self) -> Option<&Result<GraphCommandOutcome, ViewSessionError>> {
        match self {
            Self::Command { outcome } => Some(outcome),
            Self::Selection { .. } | Self::Viewport { .. } | Self::Layout { .. } => None,
        }
    }

    /// Consumes this receipt when it represents a command operation.
    pub fn into_command_outcome(self) -> Option<Result<GraphCommandOutcome, ViewSessionError>> {
        match self {
            Self::Command { outcome } => Some(*outcome),
            Self::Selection { .. } | Self::Viewport { .. } | Self::Layout { .. } => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayoutPublicationReceipt {
    before_revision: u64,
    after_revision: u64,
    published_nodes: Vec<NodeId>,
    withdrawn_nodes: Vec<NodeId>,
    spatial_updates: u64,
    route_recomputations: u64,
}

impl LayoutPublicationReceipt {
    pub(crate) fn new(
        before_revision: u64,
        after_revision: u64,
        published_nodes: Vec<NodeId>,
        withdrawn_nodes: Vec<NodeId>,
        spatial_updates: u64,
        route_recomputations: u64,
    ) -> Self {
        Self {
            before_revision,
            after_revision,
            published_nodes,
            withdrawn_nodes,
            spatial_updates,
            route_recomputations,
        }
    }

    pub fn layout_revision(&self) -> u64 {
        self.after_revision
    }

    pub fn before_revision(&self) -> u64 {
        self.before_revision
    }

    pub fn after_revision(&self) -> u64 {
        self.after_revision
    }

    pub fn published_nodes(&self) -> &[NodeId] {
        &self.published_nodes
    }

    pub fn withdrawn_nodes(&self) -> &[NodeId] {
        &self.withdrawn_nodes
    }

    pub fn spatial_updates(&self) -> u64 {
        self.spatial_updates
    }

    pub fn route_recomputations(&self) -> u64 {
        self.route_recomputations
    }
}

#[derive(Debug)]
pub struct FeedbackReceipt {
    operations: Vec<FeedbackOperationReceipt>,
    publication: Option<LayoutPublicationReceipt>,
}

impl FeedbackReceipt {
    pub(crate) fn new(
        operations: Vec<FeedbackOperationReceipt>,
        publication: Option<LayoutPublicationReceipt>,
    ) -> Self {
        Self {
            operations,
            publication,
        }
    }

    pub fn operations(&self) -> &[FeedbackOperationReceipt] {
        &self.operations
    }

    pub fn publication(&self) -> Option<&LayoutPublicationReceipt> {
        self.publication.as_ref()
    }

    pub fn request_next_frame(&self) -> bool {
        self.publication
            .as_ref()
            .is_some_and(|publication| publication.route_recomputations() > 0)
            || self.operations.iter().any(|receipt| {
                receipt
                    .layout_status()
                    .is_some_and(|status| matches!(status, LayoutNodeStatus::Stale))
            })
    }

    /// Consumes this feedback receipt and preserves operation ordering.
    pub fn into_operations(self) -> Vec<FeedbackOperationReceipt> {
        self.operations
    }
}
