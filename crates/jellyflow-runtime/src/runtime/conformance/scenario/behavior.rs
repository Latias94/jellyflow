use serde::{Deserialize, Serialize};

use crate::io::NodeGraphKeyCode;
use crate::runtime::connection::{CONNECT_EDGE_TRANSACTION_LABEL, ConnectEdgeRequest};
use crate::runtime::delete::DELETE_SELECTION_TRANSACTION_LABEL;
use crate::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use crate::runtime::events::{
    ConnectEnd, ConnectEndOutcome, ConnectStart, NodeDragEnd, NodeDragEndOutcome, NodeDragStart,
    NodeDragUpdate, NodeGraphGestureEvent, NodeResizeEnd, NodeResizeEndOutcome, NodeResizeStart,
    NodeResizeUpdate, ViewportMove, ViewportMoveEnd, ViewportMoveEndOutcome, ViewportMoveKind,
    ViewportMoveStart,
};
use crate::runtime::measurement::NodeMeasurement;
use crate::runtime::rendering::RenderingQueryResult;
use crate::runtime::resize::NODE_RESIZE_TRANSACTION_LABEL;
use crate::runtime::selection::SelectionBoxInput;
use crate::runtime::viewport::{ViewportDragPanInput, ViewportGestureContext, ViewportTransform};
use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_core::core::{CanvasPoint, CanvasSize, EdgeId, GroupId, NodeId};
use keyboard_types::Code as KeyCode;

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
    SelectionBox(ConformanceSelectionBoxContract),
    DeleteSelection(ConformanceDeleteSelectionContract),
    ViewportDragPanSession(ConformanceViewportDragPanSessionContract),
    RenderingQuery(ConformanceRenderingQueryContract),
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

    pub fn selection_box(contract: ConformanceSelectionBoxContract) -> Self {
        Self::SelectionBox(contract)
    }

    pub fn delete_selection(contract: ConformanceDeleteSelectionContract) -> Self {
        Self::DeleteSelection(contract)
    }

    pub fn viewport_drag_pan_session(contract: ConformanceViewportDragPanSessionContract) -> Self {
        Self::ViewportDragPanSession(contract)
    }

    pub fn rendering_query(contract: ConformanceRenderingQueryContract) -> Self {
        Self::RenderingQuery(contract)
    }

    pub fn layout_facts(contract: ConformanceLayoutFactsContract) -> Self {
        Self::LayoutFacts(contract)
    }

    pub(crate) fn actions(&self) -> Vec<ConformanceAction> {
        match self {
            Self::NodeDragSession(contract) => vec![contract.action()],
            Self::ConnectEdgeSession(contract) => vec![contract.action()],
            Self::NodeResizeSession(contract) => vec![contract.action()],
            Self::SelectionBox(contract) => vec![contract.action()],
            Self::DeleteSelection(contract) => vec![contract.action()],
            Self::ViewportDragPanSession(contract) => vec![contract.action()],
            Self::RenderingQuery(contract) => vec![contract.action()],
            Self::LayoutFacts(contract) => contract.actions(),
        }
    }

    pub(crate) fn expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        match self {
            Self::NodeDragSession(contract) => contract.expected_trace(),
            Self::ConnectEdgeSession(contract) => contract.expected_trace(),
            Self::NodeResizeSession(contract) => contract.expected_trace(),
            Self::SelectionBox(contract) => contract.expected_trace(),
            Self::DeleteSelection(contract) => contract.expected_trace(),
            Self::ViewportDragPanSession(contract) => contract.expected_trace(),
            Self::RenderingQuery(contract) => contract.expected_trace(),
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

/// Behavior contract for applying a marquee selection box and observing selection callbacks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceSelectionBoxContract {
    pub input: SelectionBoxInput,
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupId>,
}

impl ConformanceSelectionBoxContract {
    pub fn new(
        input: SelectionBoxInput,
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
    ) -> Self {
        Self {
            input,
            nodes: nodes.into_iter().collect(),
            edges: edges.into_iter().collect(),
            groups: Vec::new(),
        }
    }

    pub fn with_groups(mut self, groups: impl IntoIterator<Item = GroupId>) -> Self {
        self.groups = groups.into_iter().collect();
        self
    }

    fn action(&self) -> ConformanceAction {
        ConformanceAction::apply_selection_box(self.input)
    }

    fn expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        vec![
            ConformanceTraceEvent::selection(
                self.nodes.clone(),
                self.edges.clone(),
                self.groups.clone(),
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Selection {
                    nodes: self.nodes.clone(),
                    edges: self.edges.clone(),
                    groups: self.groups.clone(),
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::SelectionChange {
                nodes: self.nodes.clone(),
                edges: self.edges.clone(),
                groups: self.groups.clone(),
            }),
        ]
    }
}

fn usize_is_zero(value: &usize) -> bool {
    *value == 0
}

fn default_delete_commit_op_kinds(nodes: usize, edges: usize) -> Vec<String> {
    if nodes > 0 {
        return vec!["remove_node".to_owned()];
    }

    if edges > 0 {
        return vec!["remove_edge".to_owned()];
    }

    Vec::new()
}

/// Behavior contract for committing a delete-selection action and observing delete callbacks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceDeleteSelectionContract {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<NodeGraphKeyCode>,
    pub nodes: usize,
    pub edges: usize,
    #[serde(default, skip_serializing_if = "usize_is_zero")]
    pub groups: usize,
    #[serde(default, skip_serializing_if = "usize_is_zero")]
    pub sticky_notes: usize,
    pub commit_op_kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disconnected: Vec<EdgeConnection>,
}

impl ConformanceDeleteSelectionContract {
    pub fn new(nodes: usize, edges: usize) -> Self {
        Self {
            key: None,
            nodes,
            edges,
            groups: 0,
            sticky_notes: 0,
            commit_op_kinds: default_delete_commit_op_kinds(nodes, edges),
            disconnected: Vec::new(),
        }
    }

    pub fn for_key(mut self, key: KeyCode) -> Self {
        self.key = Some(NodeGraphKeyCode(key));
        self
    }

    pub fn with_commit_op_kinds(
        mut self,
        op_kinds: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.commit_op_kinds = op_kinds.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_deleted_groups(mut self, groups: usize) -> Self {
        self.groups = groups;
        self
    }

    pub fn with_deleted_sticky_notes(mut self, sticky_notes: usize) -> Self {
        self.sticky_notes = sticky_notes;
        self
    }

    pub fn with_disconnected(
        mut self,
        disconnected: impl IntoIterator<Item = EdgeConnection>,
    ) -> Self {
        self.disconnected = disconnected.into_iter().collect();
        self
    }

    fn action(&self) -> ConformanceAction {
        match self.key {
            Some(key) => ConformanceAction::apply_delete_selection_for_key(key.0),
            None => ConformanceAction::apply_delete_selection(),
        }
    }

    fn expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        let mut trace = vec![
            ConformanceTraceEvent::graph_commit(
                Some(DELETE_SELECTION_TRANSACTION_LABEL),
                self.commit_op_kinds.iter().map(String::as_str),
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(DELETE_SELECTION_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: self.nodes,
                edges: self.edges,
            }),
        ];

        if self.nodes > 0 {
            trace.push(ConformanceTraceEvent::callback(
                ConformanceCallbackEvent::NodesChange { count: self.nodes },
            ));
        }

        if self.edges > 0 {
            trace.push(ConformanceTraceEvent::callback(
                ConformanceCallbackEvent::EdgesChange { count: self.edges },
            ));
        }

        for connection in self.disconnected.iter().copied() {
            trace.push(ConformanceTraceEvent::callback(
                ConformanceCallbackEvent::ConnectionChange(ConnectionChange::Disconnected(
                    connection,
                )),
            ));
            trace.push(ConformanceTraceEvent::callback(
                ConformanceCallbackEvent::Disconnect(connection),
            ));
        }

        if self.nodes > 0 {
            trace.push(ConformanceTraceEvent::callback(
                ConformanceCallbackEvent::NodesDelete { count: self.nodes },
            ));
        }

        if self.edges > 0 {
            trace.push(ConformanceTraceEvent::callback(
                ConformanceCallbackEvent::EdgesDelete { count: self.edges },
            ));
        }

        if self.groups > 0 {
            trace.push(ConformanceTraceEvent::callback(
                ConformanceCallbackEvent::GroupsDelete { count: self.groups },
            ));
        }

        if self.sticky_notes > 0 {
            trace.push(ConformanceTraceEvent::callback(
                ConformanceCallbackEvent::StickyNotesDelete {
                    count: self.sticky_notes,
                },
            ));
        }

        trace.extend([
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::Delete {
                nodes: self.nodes,
                edges: self.edges,
                groups: self.groups,
                sticky_notes: self.sticky_notes,
            }),
            ConformanceTraceEvent::selection(Vec::new(), Vec::new(), Vec::new()),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Selection {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    groups: Vec::new(),
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::SelectionChange {
                nodes: Vec::new(),
                edges: Vec::new(),
                groups: Vec::new(),
            }),
        ]);

        trace
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

/// Behavior contract for reading renderer-facing order and visibility in one store query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceRenderingQueryContract {
    pub viewport_size: CanvasSize,
    pub expected: RenderingQueryResult,
}

impl ConformanceRenderingQueryContract {
    pub fn new(viewport_size: CanvasSize, expected: RenderingQueryResult) -> Self {
        Self {
            viewport_size,
            expected,
        }
    }

    fn action(&self) -> ConformanceAction {
        ConformanceAction::assert_rendering_query(self.viewport_size, self.expected.clone())
    }

    fn expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        Vec::new()
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

    pub fn with_selection_box_contract(self, contract: ConformanceSelectionBoxContract) -> Self {
        self.with_behavior(ConformanceBehavior::selection_box(contract))
    }

    pub fn with_delete_selection_contract(
        self,
        contract: ConformanceDeleteSelectionContract,
    ) -> Self {
        self.with_behavior(ConformanceBehavior::delete_selection(contract))
    }

    pub fn with_viewport_drag_pan_session_contract(
        self,
        contract: ConformanceViewportDragPanSessionContract,
    ) -> Self {
        self.with_behavior(ConformanceBehavior::viewport_drag_pan_session(contract))
    }

    pub fn with_rendering_query_contract(
        self,
        contract: ConformanceRenderingQueryContract,
    ) -> Self {
        self.with_behavior(ConformanceBehavior::rendering_query(contract))
    }

    pub fn with_layout_facts_contract(self, contract: ConformanceLayoutFactsContract) -> Self {
        self.with_behavior(ConformanceBehavior::layout_facts(contract))
    }
}
