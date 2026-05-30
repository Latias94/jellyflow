//! Headless runtime store (B-layer) for node graphs.
//!
//! This is the ergonomic "single entry point" that B-layer consumers want:
//! - authoritative `Graph` (serializable document),
//! - per-user/per-project `NodeGraphViewState` (pan/zoom/selection),
//! - undo/redo history (`GraphHistory`),
//! - dispatch methods that return full-fidelity `NodeGraphPatch` plus XyFlow-style projections.

mod dispatch;
mod events;
mod subscriptions;
mod view;

use crate::io::{
    NodeGraphEditorConfig, NodeGraphInteractionConfig, NodeGraphRuntimeTuning, NodeGraphViewState,
};
use crate::profile::{ApplyPipelineError, GraphProfile};
use crate::runtime::events::{NodeGraphGestureEvent, NodeGraphStoreEvent, SubscriptionToken};
use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::middleware::NodeGraphStoreMiddleware;
use crate::runtime::xyflow::changes::{
    ChangesToTransactionError, NodeGraphChanges, NodeGraphPatch,
};
use jellyflow_core::core::Graph;
use jellyflow_core::ops::{GraphHistory, GraphTransaction};

/// Dispatch outcome for store actions.
#[derive(Debug, Clone)]
pub struct DispatchOutcome {
    /// Full-fidelity patch that was committed.
    pub patch: NodeGraphPatch,
    /// XyFlow-style node/edge projection derived from `patch`.
    pub node_edge_changes: NodeGraphChanges,
}

impl DispatchOutcome {
    pub fn new(patch: NodeGraphPatch, node_edge_changes: NodeGraphChanges) -> Self {
        Self {
            patch,
            node_edge_changes,
        }
    }

    pub fn from_committed(committed: GraphTransaction) -> Self {
        let patch = NodeGraphPatch::new(committed);
        let node_edge_changes = patch.node_edge_changes();
        Self::new(patch, node_edge_changes)
    }

    pub fn committed(&self) -> &GraphTransaction {
        self.patch.transaction()
    }

    pub fn changes(&self) -> &NodeGraphChanges {
        &self.node_edge_changes
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error(transparent)]
    Apply(#[from] ApplyPipelineError),
    #[error(transparent)]
    Changes(#[from] ChangesToTransactionError),
}

/// Minimal B-layer store.
///
/// This is intentionally headless-safe and does not depend on `fret-ui`.
pub struct NodeGraphStore {
    graph: Graph,
    graph_revision: u64,
    view_state: NodeGraphViewState,
    interaction: NodeGraphInteractionConfig,
    runtime_tuning: NodeGraphRuntimeTuning,
    history: GraphHistory,
    profile: Option<Box<dyn GraphProfile>>,
    middleware: Option<Box<dyn NodeGraphStoreMiddleware>>,
    lookups: NodeGraphLookups,

    next_subscription: u64,
    event_subscriptions: Vec<(
        SubscriptionToken,
        Box<dyn for<'a> FnMut(NodeGraphStoreEvent<'a>)>,
    )>,
    gesture_subscriptions: Vec<(SubscriptionToken, Box<dyn FnMut(NodeGraphGestureEvent)>)>,
    selector_subscriptions: Vec<subscriptions::SelectorSubscription>,
}

enum DispatchProfile<'a> {
    StoreProfile,
    External(&'a mut dyn GraphProfile),
}

impl std::fmt::Debug for NodeGraphStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeGraphStore")
            .field("graph_id", &self.graph.graph_id)
            .field("graph_revision", &self.graph_revision)
            .field("node_count", &self.graph.nodes.len())
            .field("edge_count", &self.graph.edges.len())
            .field("lookup_node_count", &self.lookups.node_lookup.len())
            .field("lookup_edge_count", &self.lookups.edge_lookup.len())
            .field("undo_len", &self.history.undo_len())
            .field("redo_len", &self.history.redo_len())
            .field("has_profile", &self.profile.is_some())
            .field("event_subscription_count", &self.event_subscriptions.len())
            .field(
                "gesture_subscription_count",
                &self.gesture_subscriptions.len(),
            )
            .field(
                "selector_subscription_count",
                &self.selector_subscriptions.len(),
            )
            .finish()
    }
}

impl NodeGraphStore {
    /// Creates a store with an explicit editor configuration payload.
    pub fn new(
        graph: Graph,
        mut view_state: NodeGraphViewState,
        editor_config: NodeGraphEditorConfig,
    ) -> Self {
        view_state.sanitize_for_graph(&graph);
        let mut lookups = NodeGraphLookups::default();
        lookups.rebuild_from(&graph);
        Self {
            graph,
            graph_revision: 0,
            view_state,
            interaction: editor_config.interaction,
            runtime_tuning: editor_config.runtime_tuning,
            history: GraphHistory::default(),
            profile: None,
            middleware: None,
            lookups,
            next_subscription: 1,
            event_subscriptions: Vec::new(),
            gesture_subscriptions: Vec::new(),
            selector_subscriptions: Vec::new(),
        }
    }

    /// Creates a store with a profile pipeline (apply -> concretize -> validate).
    pub fn with_profile(
        graph: Graph,
        view_state: NodeGraphViewState,
        editor_config: NodeGraphEditorConfig,
        profile: Box<dyn GraphProfile>,
    ) -> Self {
        let mut view_state = view_state;
        view_state.sanitize_for_graph(&graph);
        let mut lookups = NodeGraphLookups::default();
        lookups.rebuild_from(&graph);
        Self {
            graph,
            graph_revision: 0,
            view_state,
            interaction: editor_config.interaction,
            runtime_tuning: editor_config.runtime_tuning,
            history: GraphHistory::default(),
            profile: Some(profile),
            middleware: None,
            lookups,
            next_subscription: 1,
            event_subscriptions: Vec::new(),
            gesture_subscriptions: Vec::new(),
            selector_subscriptions: Vec::new(),
        }
    }

    pub fn with_middleware(mut self, middleware: impl NodeGraphStoreMiddleware) -> Self {
        self.middleware = Some(Box::new(middleware));
        self
    }
}
