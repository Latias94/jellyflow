use serde::{Deserialize, Serialize};

use crate::runtime::connection::{CONNECT_EDGE_TRANSACTION_LABEL, ConnectEdgeRequest};
use crate::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use crate::runtime::events::{
    ConnectEnd, ConnectEndOutcome, ConnectStart, NodeDragEnd, NodeDragEndOutcome, NodeDragStart,
    NodeDragUpdate, NodeGraphGestureEvent, NodeResizeEnd, NodeResizeEndOutcome, NodeResizeStart,
    NodeResizeUpdate, ViewportMove, ViewportMoveEnd, ViewportMoveEndOutcome, ViewportMoveKind,
    ViewportMoveStart,
};
use crate::runtime::measurement::NodeMeasurement;
use crate::runtime::resize::NODE_RESIZE_TRANSACTION_LABEL;
use crate::runtime::viewport::{ViewportDragPanInput, ViewportGestureContext, ViewportTransform};
use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_core::core::{CanvasPoint, CanvasSize, NodeId};

use super::action::{
    ConformanceAction, ConformanceLayoutFactsExpectation, ConformanceNodePointerResizeRequest,
};
use super::suite::ConformanceScenario;
use super::trace::{ConformanceCallbackEvent, ConformanceTraceEvent, ConformanceViewChange};

/// High-level conformance behavior that expands to runtime actions and expected trace events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConformanceBehavior {
    NodeDragSession(ConformanceNodeDragSessionContract),
    ConnectEdgeSession(ConformanceConnectEdgeSessionContract),
    NodeResizeSession(ConformanceNodeResizeSessionContract),
    ViewportDragPanSession(ConformanceViewportDragPanSessionContract),
    LayoutFacts(ConformanceLayoutFactsContract),
}

impl ConformanceBehavior {
    pub fn node_drag_session(contract: ConformanceNodeDragSessionContract) -> Self {
        Self::NodeDragSession(contract)
    }

    pub fn connect_edge_session(contract: ConformanceConnectEdgeSessionContract) -> Self {
        Self::ConnectEdgeSession(contract)
    }

    pub fn node_resize_session(contract: ConformanceNodeResizeSessionContract) -> Self {
        Self::NodeResizeSession(contract)
    }

    pub fn viewport_drag_pan_session(contract: ConformanceViewportDragPanSessionContract) -> Self {
        Self::ViewportDragPanSession(contract)
    }

    pub fn layout_facts(contract: ConformanceLayoutFactsContract) -> Self {
        Self::LayoutFacts(contract)
    }

    pub(crate) fn actions(&self) -> Vec<ConformanceAction> {
        match self {
            Self::NodeDragSession(contract) => vec![contract.action()],
            Self::ConnectEdgeSession(contract) => vec![contract.action()],
            Self::NodeResizeSession(contract) => vec![contract.action()],
            Self::ViewportDragPanSession(contract) => vec![contract.action()],
            Self::LayoutFacts(contract) => contract.actions(),
        }
    }

    pub(crate) fn expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        match self {
            Self::NodeDragSession(contract) => contract.expected_trace(),
            Self::ConnectEdgeSession(contract) => contract.expected_trace(),
            Self::NodeResizeSession(contract) => contract.expected_trace(),
            Self::ViewportDragPanSession(contract) => contract.expected_trace(),
            Self::LayoutFacts(contract) => contract.expected_trace(),
        }
    }
}

/// Behavior contract for a committed node drag session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceNodeDragSessionContract {
    pub primary: NodeId,
    pub nodes: Vec<NodeId>,
    pub start: CanvasPoint,
    pub to: CanvasPoint,
    pub commit_op_kinds: Vec<String>,
}

impl ConformanceNodeDragSessionContract {
    pub fn new(primary: NodeId, start: CanvasPoint, to: CanvasPoint) -> Self {
        Self {
            primary,
            nodes: vec![primary],
            start,
            to,
            commit_op_kinds: vec!["set_node_pos".to_owned()],
        }
    }

    pub fn with_nodes(mut self, nodes: impl IntoIterator<Item = NodeId>) -> Self {
        self.nodes = nodes.into_iter().collect();
        self
    }

    pub fn with_commit_op_kinds(
        mut self,
        op_kinds: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.commit_op_kinds = op_kinds.into_iter().map(Into::into).collect();
        self
    }

    fn action(&self) -> ConformanceAction {
        ConformanceAction::apply_node_drag_session(self.primary, self.start, self.to)
    }

    fn expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        let start = NodeDragStart {
            primary: self.primary,
            nodes: self.nodes.clone(),
            pointer: self.start,
        };
        let update = NodeDragUpdate {
            primary: self.primary,
            nodes: self.nodes.clone(),
            pointer: self.to,
        };
        let end = NodeDragEnd {
            primary: self.primary,
            nodes: self.nodes.clone(),
            pointer: self.to,
            outcome: NodeDragEndOutcome::Committed,
        };

        vec![
            ConformanceTraceEvent::gesture(NodeGraphGestureEvent::NodeDragStart(start.clone())),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDragStart(start)),
            ConformanceTraceEvent::graph_commit(
                Some(NODE_DRAG_TRANSACTION_LABEL),
                self.commit_op_kinds.iter().map(String::as_str),
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_DRAG_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: self.nodes.len(),
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange {
                count: self.nodes.len(),
            }),
            ConformanceTraceEvent::gesture(NodeGraphGestureEvent::NodeDragUpdate(update.clone())),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDrag(update)),
            ConformanceTraceEvent::gesture(NodeGraphGestureEvent::NodeDragEnd(end.clone())),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDragEnd(end)),
        ]
    }
}

/// Behavior contract for a committed connect-edge session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceConnectEdgeSessionContract {
    pub start: ConnectStart,
    pub request: ConnectEdgeRequest,
    pub connection: EdgeConnection,
}

impl ConformanceConnectEdgeSessionContract {
    pub fn new(
        start: ConnectStart,
        request: ConnectEdgeRequest,
        connection: EdgeConnection,
    ) -> Self {
        Self {
            start,
            request,
            connection,
        }
    }

    fn action(&self) -> ConformanceAction {
        ConformanceAction::apply_connect_edge_session(self.start.clone(), self.request)
    }

    fn expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        let end = ConnectEnd {
            kind: self.start.kind.clone(),
            mode: self.start.mode,
            target: Some(self.request.to),
            outcome: ConnectEndOutcome::Committed,
        };

        vec![
            ConformanceTraceEvent::gesture(NodeGraphGestureEvent::ConnectStart(self.start.clone())),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectStart(
                self.start.clone(),
            )),
            ConformanceTraceEvent::graph_commit(Some(CONNECT_EDGE_TRANSACTION_LABEL), ["add_edge"]),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(CONNECT_EDGE_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 0,
                edges: 1,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::EdgesChange { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectionChange(
                ConnectionChange::Connected(self.connection),
            )),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::Connect(self.connection)),
            ConformanceTraceEvent::gesture(NodeGraphGestureEvent::ConnectEnd(end.clone())),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectEnd(end)),
        ]
    }
}

/// Behavior contract for a committed pointer-driven node resize session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceNodeResizeSessionContract {
    pub request: ConformanceNodePointerResizeRequest,
    pub update: NodeResizeUpdate,
    pub commit_op_kinds: Vec<String>,
}

impl ConformanceNodeResizeSessionContract {
    pub fn new(request: ConformanceNodePointerResizeRequest, update: NodeResizeUpdate) -> Self {
        Self {
            request,
            update,
            commit_op_kinds: vec!["set_node_size".to_owned()],
        }
    }

    pub fn with_commit_op_kinds(
        mut self,
        op_kinds: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.commit_op_kinds = op_kinds.into_iter().map(Into::into).collect();
        self
    }

    fn action(&self) -> ConformanceAction {
        ConformanceAction::apply_node_pointer_resize_session(self.request.into_runtime())
    }

    fn expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        let start = NodeResizeStart {
            node: self.request.node,
            direction: self.request.direction.into_runtime(),
            pointer: self.request.start,
        };
        let end = NodeResizeEnd {
            node: self.request.node,
            direction: self.request.direction.into_runtime(),
            pointer: self.request.current,
            outcome: NodeResizeEndOutcome::Committed,
        };

        vec![
            ConformanceTraceEvent::gesture(NodeGraphGestureEvent::NodeResizeStart(start.clone())),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeResizeStart(start)),
            ConformanceTraceEvent::graph_commit(
                Some(NODE_RESIZE_TRANSACTION_LABEL),
                self.commit_op_kinds.iter().map(String::as_str),
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_RESIZE_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
            ConformanceTraceEvent::gesture(NodeGraphGestureEvent::NodeResizeUpdate(
                self.update.clone(),
            )),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeResize(
                self.update.clone(),
            )),
            ConformanceTraceEvent::gesture(NodeGraphGestureEvent::NodeResizeEnd(end.clone())),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeResizeEnd(end)),
        ]
    }
}

/// Behavior contract for an accepted viewport drag-pan session.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConformanceViewportDragPanSessionContract {
    pub context: ViewportGestureContext,
    pub input: ViewportDragPanInput,
    pub start: ViewportTransform,
    pub end: ViewportTransform,
}

impl ConformanceViewportDragPanSessionContract {
    pub fn new(
        context: ViewportGestureContext,
        input: ViewportDragPanInput,
        start: ViewportTransform,
        end: ViewportTransform,
    ) -> Self {
        Self {
            context,
            input,
            start,
            end,
        }
    }

    fn action(&self) -> ConformanceAction {
        ConformanceAction::apply_viewport_drag_pan_session(self.context, self.input)
    }

    fn expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        let start = ViewportMoveStart {
            kind: ViewportMoveKind::PanDrag,
            pan: self.start.pan,
            zoom: self.start.zoom,
        };
        let update = ViewportMove {
            kind: ViewportMoveKind::PanDrag,
            pan: self.end.pan,
            zoom: self.end.zoom,
        };
        let end = ViewportMoveEnd {
            kind: ViewportMoveKind::PanDrag,
            pan: self.end.pan,
            zoom: self.end.zoom,
            outcome: ViewportMoveEndOutcome::Ended,
        };

        vec![
            ConformanceTraceEvent::gesture(NodeGraphGestureEvent::ViewportMoveStart(start)),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMoveStart(start)),
            ConformanceTraceEvent::viewport(self.end.pan, self.end.zoom),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport {
                    pan: self.end.pan,
                    zoom: self.end.zoom,
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan: self.end.pan,
                zoom: self.end.zoom,
            }),
            ConformanceTraceEvent::gesture(NodeGraphGestureEvent::ViewportMove(update)),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMove(update)),
            ConformanceTraceEvent::gesture(NodeGraphGestureEvent::ViewportMoveEnd(end)),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMoveEnd(end)),
        ]
    }
}

/// Behavior contract for reporting measurements once and reading derived layout facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceLayoutFactsContract {
    pub measurements: Vec<NodeMeasurement>,
    pub viewport_size: CanvasSize,
    pub expected: ConformanceLayoutFactsExpectation,
}

impl ConformanceLayoutFactsContract {
    pub fn new(
        measurements: impl IntoIterator<Item = NodeMeasurement>,
        viewport_size: CanvasSize,
        expected: ConformanceLayoutFactsExpectation,
    ) -> Self {
        Self {
            measurements: measurements.into_iter().collect(),
            viewport_size,
            expected,
        }
    }

    fn actions(&self) -> Vec<ConformanceAction> {
        self.measurements
            .iter()
            .cloned()
            .map(ConformanceAction::report_node_measurement)
            .chain([ConformanceAction::assert_layout_facts(
                self.viewport_size,
                self.expected.clone(),
            )])
            .collect()
    }

    fn expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        Vec::new()
    }
}

impl ConformanceScenario {
    pub fn with_node_drag_session_contract(
        self,
        contract: ConformanceNodeDragSessionContract,
    ) -> Self {
        self.with_behavior(ConformanceBehavior::node_drag_session(contract))
    }

    pub fn with_connect_edge_session_contract(
        self,
        contract: ConformanceConnectEdgeSessionContract,
    ) -> Self {
        self.with_behavior(ConformanceBehavior::connect_edge_session(contract))
    }

    pub fn with_node_resize_session_contract(
        self,
        contract: ConformanceNodeResizeSessionContract,
    ) -> Self {
        self.with_behavior(ConformanceBehavior::node_resize_session(contract))
    }

    pub fn with_viewport_drag_pan_session_contract(
        self,
        contract: ConformanceViewportDragPanSessionContract,
    ) -> Self {
        self.with_behavior(ConformanceBehavior::viewport_drag_pan_session(contract))
    }

    pub fn with_layout_facts_contract(self, contract: ConformanceLayoutFactsContract) -> Self {
        self.with_behavior(ConformanceBehavior::layout_facts(contract))
    }
}
