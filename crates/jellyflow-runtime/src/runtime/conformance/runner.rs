use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::drag::NodeDragRequest;
use crate::runtime::events::{
    ConnectEnd, ConnectStart, NodeDragEnd, NodeDragStart, NodeDragUpdate, NodeGraphStoreEvent,
    ViewChange, ViewportMove, ViewportMoveEnd, ViewportMoveStart,
};
use crate::runtime::store::NodeGraphStore;
use crate::runtime::xyflow::callbacks::{
    ConnectionChange, EdgeConnection, NodeGraphCommitCallbacks, NodeGraphGestureCallbacks,
    NodeGraphViewCallbacks, SelectionChange, install_callbacks,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{CanvasPoint, EdgeId};
use jellyflow_core::ops::EdgeEndpoints;

use super::reports::{ConformanceRunError, ConformanceRunReport, ConformanceSuiteReport};
use super::scenario::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceScenario, ConformanceSuite,
    ConformanceTraceConfig, ConformanceTraceEvent, ConformanceViewChange,
};

impl ConformanceSuite {
    pub fn run(&self) -> ConformanceSuiteReport {
        run_conformance_suite(self)
    }
}

pub fn run_conformance_scenario(
    scenario: &ConformanceScenario,
) -> Result<ConformanceRunReport, ConformanceRunError> {
    ConformanceRunner::new(scenario).run()
}

pub fn run_conformance_suite(suite: &ConformanceSuite) -> ConformanceSuiteReport {
    let mut scenario_reports = Vec::new();
    let mut errors = Vec::new();

    for scenario in &suite.scenarios {
        match run_conformance_scenario(scenario) {
            Ok(report) => scenario_reports.push(report),
            Err(error) => errors.push(error),
        }
    }

    ConformanceSuiteReport {
        suite: suite.name.clone(),
        scenario_reports,
        errors,
    }
}

#[derive(Debug)]
pub struct ConformanceRunner<'a> {
    scenario: &'a ConformanceScenario,
}

impl<'a> ConformanceRunner<'a> {
    pub fn new(scenario: &'a ConformanceScenario) -> Self {
        Self { scenario }
    }

    pub fn run(&self) -> Result<ConformanceRunReport, ConformanceRunError> {
        let mut store = NodeGraphStore::new(
            self.scenario.setup.graph.clone(),
            self.scenario.setup.view_state.clone(),
            self.scenario.setup.editor_config.clone(),
        );
        let trace = Rc::new(RefCell::new(Vec::new()));
        install_trace_recorders(&mut store, self.scenario.setup.trace, trace.clone());

        for (index, action) in self.scenario.actions.iter().enumerate() {
            execute_action(&mut store, action).map_err(|message| ConformanceRunError {
                scenario: self.scenario.name.clone(),
                action_index: index,
                action_kind: action.kind().to_owned(),
                message,
            })?;
        }

        Ok(ConformanceRunReport::new(
            self.scenario.name.clone(),
            trace.borrow().clone(),
            &self.scenario.expected_trace,
        ))
    }
}

fn install_trace_recorders(
    store: &mut NodeGraphStore,
    config: ConformanceTraceConfig,
    trace: Rc<RefCell<Vec<ConformanceTraceEvent>>>,
) {
    if config.record_store_events || config.record_gesture_events {
        let store_trace = trace.clone();
        let token = store.subscribe(move |event| {
            if config.record_store_events {
                store_trace
                    .borrow_mut()
                    .push(ConformanceTraceEvent::from_store_event(event));
            }
        });

        if config.record_gesture_events {
            let gesture_trace = trace.clone();
            store.subscribe_gesture_with_token(token, move |event| {
                gesture_trace
                    .borrow_mut()
                    .push(ConformanceTraceEvent::Gesture(event));
            });
        }
    }

    if config.record_xyflow_callbacks {
        let _ = install_callbacks(&mut *store, CallbackTraceRecorder { trace });
    }
}

fn execute_action(store: &mut NodeGraphStore, action: &ConformanceAction) -> Result<(), String> {
    match action {
        ConformanceAction::DispatchTransaction { transaction } => store
            .dispatch_transaction(transaction)
            .map(|_| ())
            .map_err(|err| err.to_string()),
        ConformanceAction::ApplyNodeDrag { node, to } => store
            .apply_node_drag(NodeDragRequest {
                node: *node,
                to: *to,
            })
            .map(|_| ())
            .map_err(|err| err.to_string()),
        ConformanceAction::ApplyAutoPan { request } => store
            .apply_auto_pan(*request)
            .map(|_| ())
            .ok_or_else(|| "auto-pan request was rejected".to_owned()),
        ConformanceAction::ApplyViewportPan { request } => store
            .apply_viewport_pan(*request)
            .map(|_| ())
            .ok_or_else(|| "viewport pan request was rejected".to_owned()),
        ConformanceAction::ApplyViewportZoom { request } => store
            .apply_viewport_zoom(*request)
            .map(|_| ())
            .ok_or_else(|| "viewport zoom request was rejected".to_owned()),
        ConformanceAction::SetViewport { pan, zoom } => {
            store.set_viewport(*pan, *zoom);
            Ok(())
        }
        ConformanceAction::SetSelection {
            nodes,
            edges,
            groups,
        } => {
            store.set_selection(nodes.clone(), edges.clone(), groups.clone());
            Ok(())
        }
        ConformanceAction::EmitGesture { event } => {
            store.emit_gesture(event.clone());
            Ok(())
        }
    }
}

impl ConformanceTraceEvent {
    fn from_store_event(event: NodeGraphStoreEvent<'_>) -> Self {
        match event {
            NodeGraphStoreEvent::DocumentReplaced { before, after } => Self::DocumentReplaced {
                before_revision: before.graph_revision,
                after_revision: after.graph_revision,
            },
            NodeGraphStoreEvent::GraphCommitted { patch } => Self::GraphCommitted {
                label: patch.transaction().label().map(str::to_owned),
                op_kinds: patch
                    .transaction()
                    .ops()
                    .iter()
                    .map(serialized_graph_op_kind)
                    .collect(),
            },
            NodeGraphStoreEvent::ViewChanged { changes, .. } => Self::ViewChanged {
                changes: changes
                    .iter()
                    .map(ConformanceViewChange::from_view_change)
                    .collect(),
            },
        }
    }
}

impl ConformanceViewChange {
    fn from_view_change(change: &ViewChange) -> Self {
        match change {
            ViewChange::Viewport { pan, zoom } => Self::Viewport {
                pan: *pan,
                zoom: *zoom,
            },
            ViewChange::Selection {
                nodes,
                edges,
                groups,
            } => Self::Selection {
                nodes: nodes.clone(),
                edges: edges.clone(),
                groups: groups.clone(),
            },
        }
    }
}

#[derive(Clone)]
struct CallbackTraceRecorder {
    trace: Rc<RefCell<Vec<ConformanceTraceEvent>>>,
}

impl CallbackTraceRecorder {
    fn push(&self, event: ConformanceCallbackEvent) {
        self.trace
            .borrow_mut()
            .push(ConformanceTraceEvent::Callback(event));
    }
}

impl NodeGraphCommitCallbacks for CallbackTraceRecorder {
    fn on_graph_commit(&mut self, patch: &crate::runtime::commit::NodeGraphPatch) {
        self.push(ConformanceCallbackEvent::GraphCommit {
            label: patch.transaction().label().map(str::to_owned),
        });
    }

    fn on_node_edge_changes(&mut self, changes: &NodeGraphChanges) {
        self.push(ConformanceCallbackEvent::NodeEdgeChanges {
            nodes: changes.nodes().len(),
            edges: changes.edges().len(),
        });
    }

    fn on_nodes_change(&mut self, changes: &[NodeChange]) {
        self.push(ConformanceCallbackEvent::NodesChange {
            count: changes.len(),
        });
    }

    fn on_edges_change(&mut self, changes: &[EdgeChange]) {
        self.push(ConformanceCallbackEvent::EdgesChange {
            count: changes.len(),
        });
    }

    fn on_connection_change(&mut self, change: ConnectionChange) {
        self.push(ConformanceCallbackEvent::ConnectionChange(change));
    }

    fn on_connect(&mut self, conn: EdgeConnection) {
        self.push(ConformanceCallbackEvent::Connect(conn));
    }

    fn on_disconnect(&mut self, conn: EdgeConnection) {
        self.push(ConformanceCallbackEvent::Disconnect(conn));
    }

    fn on_reconnect(&mut self, edge: EdgeId, from: EdgeEndpoints, to: EdgeEndpoints) {
        self.push(ConformanceCallbackEvent::Reconnect { edge, from, to });
    }
}

impl NodeGraphViewCallbacks for CallbackTraceRecorder {
    fn on_view_change(&mut self, changes: &[ViewChange]) {
        self.push(ConformanceCallbackEvent::ViewChange {
            changes: changes
                .iter()
                .map(ConformanceViewChange::from_view_change)
                .collect(),
        });
    }

    fn on_viewport_change(&mut self, pan: CanvasPoint, zoom: f32) {
        self.push(ConformanceCallbackEvent::ViewportChange { pan, zoom });
    }

    fn on_selection_change(&mut self, sel: SelectionChange) {
        let (nodes, edges, groups) = sel.into_parts();
        self.push(ConformanceCallbackEvent::SelectionChange {
            nodes,
            edges,
            groups,
        });
    }
}

impl NodeGraphGestureCallbacks for CallbackTraceRecorder {
    fn on_move_start(&mut self, ev: ViewportMoveStart) {
        self.push(ConformanceCallbackEvent::ViewportMoveStart(ev));
    }

    fn on_move(&mut self, ev: ViewportMove) {
        self.push(ConformanceCallbackEvent::ViewportMove(ev));
    }

    fn on_move_end(&mut self, ev: ViewportMoveEnd) {
        self.push(ConformanceCallbackEvent::ViewportMoveEnd(ev));
    }

    fn on_node_drag_start(&mut self, ev: NodeDragStart) {
        self.push(ConformanceCallbackEvent::NodeDragStart(ev));
    }

    fn on_node_drag(&mut self, ev: NodeDragUpdate) {
        self.push(ConformanceCallbackEvent::NodeDrag(ev));
    }

    fn on_node_drag_end(&mut self, ev: NodeDragEnd) {
        self.push(ConformanceCallbackEvent::NodeDragEnd(ev));
    }

    fn on_connect_start(&mut self, ev: ConnectStart) {
        self.push(ConformanceCallbackEvent::ConnectStart(ev));
    }

    fn on_connect_end(&mut self, ev: ConnectEnd) {
        self.push(ConformanceCallbackEvent::ConnectEnd(ev));
    }
}

fn serialized_graph_op_kind(op: &jellyflow_core::ops::GraphOp) -> String {
    serde_json::to_value(op)
        .ok()
        .and_then(|value| {
            value
                .get("op")
                .and_then(|op| op.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_owned())
}
