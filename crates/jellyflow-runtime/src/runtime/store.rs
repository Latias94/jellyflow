//! Headless runtime store (B-layer) for node graphs.
//!
//! This is the ergonomic "single entry point" that B-layer consumers want:
//! - authoritative `Graph` (serializable document),
//! - per-user/per-project `NodeGraphViewState` (pan/zoom/selection),
//! - undo/redo history (`GraphHistory`),
//! - dispatch methods that return a full-fidelity `NodeGraphPatch`.

mod dispatch;
mod events;
mod history;
mod snapshot;
mod subscriptions;
mod view;

use crate::io::{
    NodeGraphEditorConfig, NodeGraphInteractionConfig, NodeGraphRuntimeTuning, NodeGraphViewState,
};
use crate::profile::{ApplyPipelineError, GraphProfile, apply_transaction_with_profile};
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::events::{NodeGraphGestureEvent, NodeGraphStoreEvent, SubscriptionToken};
use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::middleware::NodeGraphStoreMiddleware;
use jellyflow_core::core::Graph;
use jellyflow_core::ops::{GraphHistory, GraphTransaction};

/// Dispatch outcome for store actions.
#[derive(Debug, Clone)]
pub struct DispatchOutcome {
    /// Full-fidelity patch that was committed.
    pub patch: NodeGraphPatch,
}

impl DispatchOutcome {
    pub fn new(patch: NodeGraphPatch) -> Self {
        Self { patch }
    }

    pub fn from_committed(committed: GraphTransaction) -> Self {
        Self::new(NodeGraphPatch::new(committed))
    }

    pub fn committed(&self) -> &GraphTransaction {
        self.patch.transaction()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error(transparent)]
    Apply(#[from] ApplyPipelineError),
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

impl DispatchProfile<'_> {
    fn apply_to_graph(
        &mut self,
        store: &mut NodeGraphStore,
        graph: &mut Graph,
        tx: &GraphTransaction,
    ) -> Result<GraphTransaction, ApplyPipelineError> {
        match self {
            Self::StoreProfile => store.apply_to_graph(graph, tx),
            Self::External(profile) => apply_transaction_with_profile(graph, &mut **profile, tx),
        }
    }
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
        view_state: NodeGraphViewState,
        editor_config: NodeGraphEditorConfig,
    ) -> Self {
        Self::new_with_optional_profile(graph, view_state, editor_config, None)
    }

    /// Creates a store with a profile pipeline (apply -> concretize -> validate).
    pub fn with_profile(
        graph: Graph,
        view_state: NodeGraphViewState,
        editor_config: NodeGraphEditorConfig,
        profile: Box<dyn GraphProfile>,
    ) -> Self {
        Self::new_with_optional_profile(graph, view_state, editor_config, Some(profile))
    }

    fn new_with_optional_profile(
        graph: Graph,
        mut view_state: NodeGraphViewState,
        editor_config: NodeGraphEditorConfig,
        profile: Option<Box<dyn GraphProfile>>,
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
            profile,
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
