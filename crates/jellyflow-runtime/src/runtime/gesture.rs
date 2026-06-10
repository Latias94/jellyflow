//! Renderer-neutral pointer and gesture session helpers.
//!
//! Adapters still own platform input capture. This module owns the runtime sequencing that should
//! stay consistent across adapters: pointer arbitration, gesture lifecycle events, and store
//! commits for common headless sessions.

use serde::{Deserialize, Serialize};

use crate::runtime::connection::{
    ConnectEdgeError, ConnectEdgeRequest, ConnectionDragActivationInput, ConnectionHandleRef,
    connection_drag_threshold_met,
};
use crate::runtime::drag::{
    NodeDragRequest, PointerGestureClaim as DragPointerGestureClaim, PointerGestureClaimInput,
    resolve_pointer_gesture_claim,
};
use crate::runtime::events::{
    ConnectEnd, ConnectEndOutcome, ConnectStart, NodeDragEnd, NodeDragEndOutcome, NodeDragStart,
    NodeDragUpdate, NodeGraphGestureEvent, ViewportMove, ViewportMoveEnd, ViewportMoveEndOutcome,
    ViewportMoveKind, ViewportMoveStart,
};
use crate::runtime::selection::{
    SelectionPointerClaim, SelectionPointerClaimInput, resolve_selection_pointer_claim,
};
use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};
use crate::runtime::viewport::{
    ViewportDragPanInput, ViewportGestureContext, ViewportGestureIntent, ViewportGestureRejection,
    ViewportPointerButton, ViewportTransform, resolve_viewport_drag_pan_gesture,
};
use jellyflow_core::core::{CanvasPoint, NodeId};

/// Adapter-normalized pointer target for a possible runtime session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum PointerSessionTarget {
    Node(NodeId),
    ConnectionHandle(ConnectionHandleRef),
    Pane { button: ViewportPointerButton },
}

/// Input for resolving which high-level runtime session should claim a pointer drag.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointerSessionClaimInput {
    pub target: PointerSessionTarget,
    pub screen_delta: CanvasPoint,
    pub context: ViewportGestureContext,
}

impl PointerSessionClaimInput {
    pub fn new(
        target: PointerSessionTarget,
        screen_delta: CanvasPoint,
        context: ViewportGestureContext,
    ) -> Self {
        Self {
            target,
            screen_delta,
            context,
        }
    }
}

/// Runtime session that should own the current pointer drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerSessionClaim {
    None,
    Selection,
    Connection,
    NodeDrag,
    ViewportPan,
}

/// One headless node-drag session from pointer start to final pointer update.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeDragSession {
    pub node: NodeId,
    pub start: CanvasPoint,
    pub to: CanvasPoint,
}

impl NodeDragSession {
    pub fn new(node: NodeId, start: CanvasPoint, to: CanvasPoint) -> Self {
        Self { node, start, to }
    }
}

/// One headless connection session that commits a new edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectEdgeSession {
    pub start: ConnectStart,
    pub request: ConnectEdgeRequest,
}

impl ConnectEdgeSession {
    pub fn new(start: ConnectStart, request: ConnectEdgeRequest) -> Self {
        Self { start, request }
    }
}

/// Outcome of applying a connection session.
#[derive(Debug, Clone)]
pub struct ConnectSessionOutcome {
    pub end_outcome: ConnectEndOutcome,
    pub committed_update: Option<DispatchOutcome>,
}

impl ConnectSessionOutcome {
    fn committed(committed_update: DispatchOutcome) -> Self {
        Self {
            end_outcome: ConnectEndOutcome::Committed,
            committed_update: Some(committed_update),
        }
    }

    fn without_commit(end_outcome: ConnectEndOutcome) -> Self {
        Self {
            end_outcome,
            committed_update: None,
        }
    }

    pub fn committed_update(&self) -> Option<&DispatchOutcome> {
        self.committed_update.as_ref()
    }
}

/// Outcome of applying a node-drag session.
#[derive(Debug, Clone)]
pub struct NodeDragSessionOutcome {
    pub nodes: Vec<NodeId>,
    pub end_outcome: NodeDragEndOutcome,
    pub committed_update: Option<DispatchOutcome>,
}

impl NodeDragSessionOutcome {
    fn committed(nodes: Vec<NodeId>, committed_update: DispatchOutcome) -> Self {
        Self {
            nodes,
            end_outcome: NodeDragEndOutcome::Committed,
            committed_update: Some(committed_update),
        }
    }

    fn without_commit(nodes: Vec<NodeId>, end_outcome: NodeDragEndOutcome) -> Self {
        Self {
            nodes,
            end_outcome,
            committed_update: None,
        }
    }

    pub fn committed_update(&self) -> Option<&DispatchOutcome> {
        self.committed_update.as_ref()
    }
}

/// One accepted viewport drag-pan session.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportDragPanSession {
    pub context: ViewportGestureContext,
    pub input: ViewportDragPanInput,
}

impl ViewportDragPanSession {
    pub fn new(context: ViewportGestureContext, input: ViewportDragPanInput) -> Self {
        Self { context, input }
    }
}

/// Outcome of applying a viewport gesture session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportGestureSessionOutcome {
    pub kind: ViewportMoveKind,
    pub transform: ViewportTransform,
}

impl NodeGraphStore {
    /// Resolves which high-level session should own a normalized pointer drag.
    pub fn resolve_pointer_session_claim(
        &self,
        input: PointerSessionClaimInput,
    ) -> PointerSessionClaim {
        if input.context.connection_in_progress {
            return PointerSessionClaim::Connection;
        }
        if input.context.user_selection_active {
            return PointerSessionClaim::Selection;
        }

        let interaction = self.resolved_interaction_state();
        let pan = interaction.pan_interaction();
        let selection_claim = resolve_selection_pointer_claim(SelectionPointerClaimInput::new(
            input.screen_delta,
            pan.pane_click_distance,
            input.context.selection_key_pressed,
            input.context.user_selection_active,
        ));
        if selection_claim != SelectionPointerClaim::Unclaimed {
            return PointerSessionClaim::Selection;
        }

        match input.target {
            PointerSessionTarget::Node(_) => {
                if pointer_claim_reaches_node_drag(PointerGestureClaimInput::new(
                    input.screen_delta,
                    false,
                    false,
                    false,
                    pan.pane_click_distance,
                    interaction.node_drag_interaction().node_drag_threshold,
                )) {
                    PointerSessionClaim::NodeDrag
                } else {
                    PointerSessionClaim::None
                }
            }
            PointerSessionTarget::ConnectionHandle(handle) => {
                if self.pointer_target_can_start_connection(handle)
                    && connection_drag_threshold_met(ConnectionDragActivationInput::new(
                        input.screen_delta,
                        interaction
                            .connection_interaction()
                            .connection_drag_threshold,
                    ))
                {
                    PointerSessionClaim::Connection
                } else {
                    PointerSessionClaim::None
                }
            }
            PointerSessionTarget::Pane { button } => {
                let result = resolve_viewport_drag_pan_gesture(
                    &pan,
                    input.context,
                    ViewportDragPanInput::new(button, input.screen_delta),
                );
                match result {
                    Ok(_) => PointerSessionClaim::ViewportPan,
                    Err(ViewportGestureRejection::UserSelectionActive) => {
                        PointerSessionClaim::Selection
                    }
                    Err(ViewportGestureRejection::ConnectionInProgress) => {
                        PointerSessionClaim::Connection
                    }
                    Err(_) => PointerSessionClaim::None,
                }
            }
        }
    }

    fn pointer_target_can_start_connection(&self, handle: ConnectionHandleRef) -> bool {
        let Some(node) = self.graph().nodes.get(&handle.node) else {
            return false;
        };
        if node.hidden || !node.ports.contains(&handle.port) {
            return false;
        }

        let Some(port) = self.graph().ports.get(&handle.port) else {
            return false;
        };
        if port.node != handle.node || port.dir != handle.direction {
            return false;
        }

        self.resolved_interaction_state()
            .port_interaction_policy(node, port)
            .can_start_connection()
    }

    /// Applies a full node-drag gesture session through gesture events and normal store dispatch.
    pub fn apply_node_drag_session(
        &mut self,
        session: NodeDragSession,
    ) -> Result<NodeDragSessionOutcome, DispatchError> {
        let plan = self.plan_node_drag(NodeDragRequest {
            node: session.node,
            to: session.to,
        });
        let nodes = plan
            .as_ref()
            .map(|plan| plan.items().iter().map(|item| item.node).collect())
            .unwrap_or_else(|| vec![session.node]);

        self.emit_gesture(NodeGraphGestureEvent::NodeDragStart(NodeDragStart {
            primary: session.node,
            nodes: nodes.clone(),
            pointer: session.start,
        }));

        let Some(plan) = plan else {
            let end_outcome = self.rejected_or_noop_node_drag_outcome(session);
            self.emit_gesture(NodeGraphGestureEvent::NodeDragEnd(NodeDragEnd {
                primary: session.node,
                nodes: nodes.clone(),
                pointer: session.to,
                outcome: end_outcome,
            }));
            return Ok(NodeDragSessionOutcome::without_commit(nodes, end_outcome));
        };

        match self.dispatch_transaction(plan.transaction()) {
            Ok(committed_update) => {
                self.emit_gesture(NodeGraphGestureEvent::NodeDragUpdate(NodeDragUpdate {
                    primary: session.node,
                    nodes: nodes.clone(),
                    pointer: session.to,
                }));
                self.emit_gesture(NodeGraphGestureEvent::NodeDragEnd(NodeDragEnd {
                    primary: session.node,
                    nodes: nodes.clone(),
                    pointer: session.to,
                    outcome: NodeDragEndOutcome::Committed,
                }));
                Ok(NodeDragSessionOutcome::committed(nodes, committed_update))
            }
            Err(err) => {
                self.emit_gesture(NodeGraphGestureEvent::NodeDragEnd(NodeDragEnd {
                    primary: session.node,
                    nodes,
                    pointer: session.to,
                    outcome: NodeDragEndOutcome::Rejected,
                }));
                Err(err)
            }
        }
    }

    /// Applies a full connect gesture session through gesture events and normal store dispatch.
    pub fn apply_connect_edge_session(
        &mut self,
        session: ConnectEdgeSession,
    ) -> Result<ConnectSessionOutcome, ConnectEdgeError> {
        self.emit_gesture(NodeGraphGestureEvent::ConnectStart(session.start.clone()));

        match self.apply_connect_edge(session.request) {
            Ok(Some(committed_update)) => {
                self.emit_gesture(NodeGraphGestureEvent::ConnectEnd(ConnectEnd {
                    kind: session.start.kind,
                    mode: session.start.mode,
                    target: Some(session.request.to),
                    outcome: ConnectEndOutcome::Committed,
                }));
                Ok(ConnectSessionOutcome::committed(committed_update))
            }
            Ok(None) => {
                self.emit_gesture(NodeGraphGestureEvent::ConnectEnd(ConnectEnd {
                    kind: session.start.kind,
                    mode: session.start.mode,
                    target: Some(session.request.to),
                    outcome: ConnectEndOutcome::NoOp,
                }));
                Ok(ConnectSessionOutcome::without_commit(
                    ConnectEndOutcome::NoOp,
                ))
            }
            Err(err) => {
                self.emit_gesture(NodeGraphGestureEvent::ConnectEnd(ConnectEnd {
                    kind: session.start.kind,
                    mode: session.start.mode,
                    target: Some(session.request.to),
                    outcome: ConnectEndOutcome::Rejected,
                }));
                Err(err)
            }
        }
    }

    /// Applies a full viewport drag-pan gesture session through gesture events and view-state.
    pub fn apply_viewport_drag_pan_session(
        &mut self,
        session: ViewportDragPanSession,
    ) -> Result<ViewportGestureSessionOutcome, ViewportGestureRejection> {
        let interaction = self.resolved_interaction_state();
        let intent = resolve_viewport_drag_pan_gesture(
            &interaction.pan_interaction(),
            session.context,
            session.input,
        )?;
        self.apply_viewport_gesture_session(intent)
    }

    fn apply_viewport_gesture_session(
        &mut self,
        intent: ViewportGestureIntent,
    ) -> Result<ViewportGestureSessionOutcome, ViewportGestureRejection> {
        let kind = intent.move_kind();
        let start = ViewportTransform::from_view_state(self.view_state())
            .ok_or(ViewportGestureRejection::InvalidInput)?;
        self.emit_gesture(NodeGraphGestureEvent::ViewportMoveStart(
            ViewportMoveStart {
                kind,
                pan: start.pan,
                zoom: start.zoom,
            },
        ));

        if !intent.apply_to_store(self) {
            self.emit_gesture(NodeGraphGestureEvent::ViewportMoveEnd(ViewportMoveEnd {
                kind,
                pan: start.pan,
                zoom: start.zoom,
                outcome: ViewportMoveEndOutcome::Canceled,
            }));
            return Err(ViewportGestureRejection::InvalidInput);
        }

        let transform = ViewportTransform::from_view_state(self.view_state())
            .ok_or(ViewportGestureRejection::InvalidInput)?;
        self.emit_gesture(NodeGraphGestureEvent::ViewportMove(ViewportMove {
            kind,
            pan: transform.pan,
            zoom: transform.zoom,
        }));
        self.emit_gesture(NodeGraphGestureEvent::ViewportMoveEnd(ViewportMoveEnd {
            kind,
            pan: transform.pan,
            zoom: transform.zoom,
            outcome: ViewportMoveEndOutcome::Ended,
        }));

        Ok(ViewportGestureSessionOutcome { kind, transform })
    }

    fn rejected_or_noop_node_drag_outcome(&self, session: NodeDragSession) -> NodeDragEndOutcome {
        let Some(node) = self.graph().nodes.get(&session.node) else {
            return NodeDragEndOutcome::Rejected;
        };
        if node.hidden || !session.to.is_finite() {
            return NodeDragEndOutcome::Rejected;
        }
        if node.pos == session.to {
            NodeDragEndOutcome::NoOp
        } else {
            NodeDragEndOutcome::Rejected
        }
    }
}

fn pointer_claim_reaches_node_drag(input: PointerGestureClaimInput) -> bool {
    matches!(
        resolve_pointer_gesture_claim(input),
        DragPointerGestureClaim::NodeDrag
    )
}
