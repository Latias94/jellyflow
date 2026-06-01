use std::cell::RefCell;
use std::rc::Rc;

use crate::io::NodeGraphViewState;
use crate::runtime::events::{NodeGraphGestureEvent, SubscriptionToken};
use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};
use crate::runtime::xyflow::callbacks::install_callbacks;
use jellyflow_core::core::{CanvasPoint, EdgeId, Graph, GroupId, NodeId};
use jellyflow_core::ops::GraphTransaction;

use super::super::fixtures::default_editor_config;
use super::events::HarnessEvent;
use super::recorder::CallbackTraceRecorder;

pub(in crate::runtime::tests) struct InteractionHarness {
    scenario: String,
    store: NodeGraphStore,
    events: Rc<RefCell<Vec<HarnessEvent>>>,
    _token: SubscriptionToken,
}

impl InteractionHarness {
    pub(in crate::runtime::tests) fn new(scenario: impl Into<String>, graph: Graph) -> Self {
        Self::with_view_state(scenario, graph, NodeGraphViewState::default())
    }

    pub(in crate::runtime::tests) fn with_view_state(
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

    pub(in crate::runtime::tests) fn store(&self) -> &NodeGraphStore {
        &self.store
    }

    pub(in crate::runtime::tests) fn store_mut(&mut self) -> &mut NodeGraphStore {
        &mut self.store
    }

    pub(in crate::runtime::tests) fn install_callback_trace(&mut self) -> SubscriptionToken {
        install_callbacks(
            &mut self.store,
            CallbackTraceRecorder::new(self.events.clone()),
        )
    }

    pub(in crate::runtime::tests) fn dispatch_transaction(
        &mut self,
        tx: &GraphTransaction,
    ) -> Result<DispatchOutcome, DispatchError> {
        self.store.dispatch_transaction(tx)
    }

    pub(in crate::runtime::tests) fn set_viewport(&mut self, pan: CanvasPoint, zoom: f32) {
        self.store.set_viewport(pan, zoom);
    }

    pub(in crate::runtime::tests) fn set_selection(
        &mut self,
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    ) {
        self.store.set_selection(nodes, edges, groups);
    }

    pub(in crate::runtime::tests) fn emit_gesture(&mut self, event: NodeGraphGestureEvent) {
        self.store.emit_gesture(event);
    }

    pub(in crate::runtime::tests) fn assert_events(&self, expected: &[HarnessEvent]) {
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
