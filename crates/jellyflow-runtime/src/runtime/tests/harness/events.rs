pub(in crate::runtime::tests) use crate::runtime::conformance::ConformanceCallbackEvent as HarnessCallbackEvent;
use crate::runtime::conformance::{ConformanceTraceEvent, ConformanceViewChange};
use crate::runtime::events::{NodeGraphGestureEvent, NodeGraphStoreEvent};
use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId};

#[derive(Debug, Clone, PartialEq)]
pub(in crate::runtime::tests) enum HarnessEvent {
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
    Callback(HarnessCallbackEvent),
}

impl HarnessEvent {
    pub(in crate::runtime::tests) fn graph_commit(label: Option<&str>, op_kinds: &[&str]) -> Self {
        Self::GraphCommitted {
            label: label.map(str::to_owned),
            op_kinds: op_kinds.iter().map(|kind| (*kind).to_owned()).collect(),
        }
    }

    pub(in crate::runtime::tests) fn viewport(pan: CanvasPoint, zoom: f32) -> Self {
        Self::ViewChanged {
            changes: vec![HarnessViewChange::Viewport { pan, zoom }],
        }
    }

    pub(in crate::runtime::tests) fn selection(
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    ) -> Self {
        Self::ViewChanged {
            changes: vec![HarnessViewChange::Selection {
                nodes,
                edges,
                groups,
            }],
        }
    }

    pub(in crate::runtime::tests) fn gesture(event: NodeGraphGestureEvent) -> Self {
        Self::Gesture(event)
    }

    pub(in crate::runtime::tests) fn callback(event: HarnessCallbackEvent) -> Self {
        Self::Callback(event)
    }

    pub(super) fn from_store_event(event: NodeGraphStoreEvent<'_>) -> Self {
        match ConformanceTraceEvent::from_store_event(event) {
            ConformanceTraceEvent::DocumentReplaced {
                before_revision,
                after_revision,
            } => Self::DocumentReplaced {
                before_revision,
                after_revision,
            },
            ConformanceTraceEvent::GraphCommitted { label, op_kinds } => {
                Self::GraphCommitted { label, op_kinds }
            }
            ConformanceTraceEvent::ViewChanged { changes } => Self::ViewChanged {
                changes: changes
                    .into_iter()
                    .map(HarnessViewChange::from_conformance_view_change)
                    .collect(),
            },
            ConformanceTraceEvent::Gesture(event) => Self::Gesture(event),
            ConformanceTraceEvent::Callback(_) => {
                unreachable!("store event projection cannot produce callback trace events")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::runtime::tests) enum HarnessViewChange {
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
    fn from_conformance_view_change(change: ConformanceViewChange) -> Self {
        match change {
            ConformanceViewChange::Viewport { pan, zoom } => Self::Viewport { pan, zoom },
            ConformanceViewChange::Selection {
                nodes,
                edges,
                groups,
            } => Self::Selection {
                nodes,
                edges,
                groups,
            },
        }
    }
}
