use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::events::{NodeGraphStoreEvent, ViewChange};
use crate::runtime::store::NodeGraphStore;
use crate::runtime::xyflow::callbacks::install_callbacks;

use super::super::scenario::{
    ConformanceTraceConfig, ConformanceTraceEvent, ConformanceViewChange,
};
use super::callbacks::CallbackTraceRecorder;

pub(super) fn install_trace_recorders(
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
        let _ = install_callbacks(&mut *store, CallbackTraceRecorder::new(trace));
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
    pub(super) fn from_view_change(change: &ViewChange) -> Self {
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
