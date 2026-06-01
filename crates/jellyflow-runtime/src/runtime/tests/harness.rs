use std::cell::RefCell;
use std::rc::Rc;

use crate::io::NodeGraphViewState;
use crate::runtime::events::{
    NodeGraphGestureEvent, NodeGraphStoreEvent, SubscriptionToken, ViewChange,
};
use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};
use jellyflow_core::core::{CanvasPoint, EdgeId, Graph, GroupId, NodeId};
use jellyflow_core::ops::GraphTransaction;

use super::fixtures::default_editor_config;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum HarnessEvent {
    DocumentReplaced {
        before_revision: u64,
        after_revision: u64,
    },
    GraphCommitted {
        label: Option<String>,
        op_kinds: Vec<String>,
    },
    ViewChanged {
        changes: Vec<HarnessViewChange>,
    },
    Gesture(NodeGraphGestureEvent),
}

impl HarnessEvent {
    pub(super) fn graph_commit(label: Option<&str>, op_kinds: &[&str]) -> Self {
        Self::GraphCommitted {
            label: label.map(str::to_owned),
            op_kinds: op_kinds.iter().map(|kind| (*kind).to_owned()).collect(),
        }
    }

    pub(super) fn viewport(pan: CanvasPoint, zoom: f32) -> Self {
        Self::ViewChanged {
            changes: vec![HarnessViewChange::Viewport { pan, zoom }],
        }
    }

    pub(super) fn selection(nodes: Vec<NodeId>, edges: Vec<EdgeId>, groups: Vec<GroupId>) -> Self {
        Self::ViewChanged {
            changes: vec![HarnessViewChange::Selection {
                nodes,
                edges,
                groups,
            }],
        }
    }

    pub(super) fn gesture(event: NodeGraphGestureEvent) -> Self {
        Self::Gesture(event)
    }

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
                    .map(HarnessViewChange::from_view_change)
                    .collect(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum HarnessViewChange {
    Viewport {
        pan: CanvasPoint,
        zoom: f32,
    },
    Selection {
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    },
}

impl HarnessViewChange {
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

pub(super) struct InteractionHarness {
    scenario: String,
    store: NodeGraphStore,
    events: Rc<RefCell<Vec<HarnessEvent>>>,
    _token: SubscriptionToken,
}

impl InteractionHarness {
    pub(super) fn new(scenario: impl Into<String>, graph: Graph) -> Self {
        Self::with_view_state(scenario, graph, NodeGraphViewState::default())
    }

    pub(super) fn with_view_state(
        scenario: impl Into<String>,
        graph: Graph,
        view_state: NodeGraphViewState,
    ) -> Self {
        let mut store = NodeGraphStore::new(graph, view_state, default_editor_config());
        let events = Rc::new(RefCell::new(Vec::new()));
        let event_trace = events.clone();
        let token = store.subscribe(move |event| {
            event_trace
                .borrow_mut()
                .push(HarnessEvent::from_store_event(event));
        });
        let gesture_trace = events.clone();
        store.subscribe_gesture_with_token(token, move |event| {
            gesture_trace
                .borrow_mut()
                .push(HarnessEvent::Gesture(event));
        });

        Self {
            scenario: scenario.into(),
            store,
            events,
            _token: token,
        }
    }

    pub(super) fn store(&self) -> &NodeGraphStore {
        &self.store
    }

    pub(super) fn dispatch_transaction(
        &mut self,
        tx: &GraphTransaction,
    ) -> Result<DispatchOutcome, DispatchError> {
        self.store.dispatch_transaction(tx)
    }

    pub(super) fn set_viewport(&mut self, pan: CanvasPoint, zoom: f32) {
        self.store.set_viewport(pan, zoom);
    }

    pub(super) fn set_selection(
        &mut self,
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    ) {
        self.store.set_selection(nodes, edges, groups);
    }

    pub(super) fn emit_gesture(&mut self, event: NodeGraphGestureEvent) {
        self.store.emit_gesture(event);
    }

    pub(super) fn assert_events(&self, expected: &[HarnessEvent]) {
        let actual = self.events.borrow();
        assert_eq!(
            actual.as_slice(),
            expected,
            "interaction harness trace mismatch for scenario `{}`\nexpected:\n{:#?}\nactual:\n{:#?}",
            self.scenario,
            expected,
            actual.as_slice(),
        );
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
