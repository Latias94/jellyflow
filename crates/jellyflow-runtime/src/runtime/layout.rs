//! Headless automatic layout integration.
//!
//! This module is a thin runtime facade over `jellyflow-layout`: it turns a layout request into a
//! normal graph transaction, then lets the store's dispatch/profile pipeline apply it.

use jellyflow_core::core::Graph;
use jellyflow_core::ops::GraphTransaction;
pub use jellyflow_layout::{
    DUGONG_LAYOUT_ENGINE_ID, DugongLayoutEngine, LAYERED_DAG_LAYOUT_FAMILY_ID, LayoutContext,
    LayoutDirection, LayoutEdgeRoute, LayoutEngine, LayoutEngineCapability, LayoutEngineId,
    LayoutEngineMetadata, LayoutEngineRegistry, LayoutEngineRequest, LayoutError, LayoutFamilyId,
    LayoutFamilyMetadata, LayoutNodePosition, LayoutOptions, LayoutPresetBuilder, LayoutRequest,
    LayoutResult, LayoutScope, LayoutSpacing, MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID,
    MIND_MAP_LAYOUT_FAMILY_ID, MIND_MAP_RADIAL_LAYOUT_ENGINE_ID, MindMapFreeformLayoutEngine,
    MindMapRadialLayoutEngine, TIDY_TREE_LAYOUT_ENGINE_ID, TidyTreeLayoutEngine,
    builtin_layout_engine_registry, layout_graph_to_transaction_with_dugong,
    layout_graph_to_transaction_with_engine, layout_graph_to_transaction_with_mind_map_freeform,
    layout_graph_to_transaction_with_mind_map_radial, layout_graph_to_transaction_with_tidy_tree,
    layout_graph_with_dugong, layout_graph_with_engine, layout_graph_with_mind_map_freeform,
    layout_graph_with_mind_map_radial, layout_graph_with_tidy_tree,
};

use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};

/// Result of applying a layout engine through the store.
#[derive(Debug, Clone)]
pub struct LayoutApplyOutcome {
    pub layout: LayoutResult,
    pub dispatch: Option<DispatchOutcome>,
}

impl LayoutApplyOutcome {
    pub fn committed(&self) -> Option<&GraphTransaction> {
        self.dispatch.as_ref().map(DispatchOutcome::committed)
    }
}

/// Errors from planning or dispatching a layout engine.
#[derive(Debug, thiserror::Error)]
pub enum LayoutApplyError {
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
}

/// Result of applying a dugong layout through the store.
#[derive(Debug, Clone)]
pub struct DugongLayoutApplyOutcome {
    pub layout: LayoutResult,
    pub dispatch: Option<DispatchOutcome>,
}

impl DugongLayoutApplyOutcome {
    pub fn committed(&self) -> Option<&GraphTransaction> {
        self.dispatch.as_ref().map(DispatchOutcome::committed)
    }
}

/// Errors from planning or dispatching a dugong layout.
#[derive(Debug, thiserror::Error)]
pub enum DugongLayoutApplyError {
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
}

impl From<LayoutApplyOutcome> for DugongLayoutApplyOutcome {
    fn from(outcome: LayoutApplyOutcome) -> Self {
        Self {
            layout: outcome.layout,
            dispatch: outcome.dispatch,
        }
    }
}

impl From<LayoutApplyError> for DugongLayoutApplyError {
    fn from(error: LayoutApplyError) -> Self {
        match error {
            LayoutApplyError::Layout(error) => Self::Layout(error),
            LayoutApplyError::Dispatch(error) => Self::Dispatch(error),
        }
    }
}

/// Builds a layout context from non-persisted runtime facts already known by the store.
pub fn layout_context_from_store(store: &NodeGraphStore) -> LayoutContext {
    let measured_node_sizes = store.graph().nodes().keys().filter_map(|node| {
        store
            .node_measurement(*node)
            .and_then(|measurement| measurement.size.map(|size| (*node, size)))
    });
    let node_origin = store.resolved_interaction_state().node_origin.normalized();

    LayoutContext::new()
        .with_measured_node_sizes(measured_node_sizes)
        .with_node_origin((node_origin.x, node_origin.y))
}

/// Runs a selected layout engine for a graph without mutating runtime state.
pub fn plan_layout(
    graph: &Graph,
    request: &LayoutEngineRequest,
    registry: &LayoutEngineRegistry,
    context: &LayoutContext,
) -> Result<LayoutResult, LayoutError> {
    layout_graph_with_engine(graph, request, registry, context)
}

/// Runs a selected layout engine and returns the transaction that would move changed nodes.
pub fn layout_transaction(
    graph: &Graph,
    request: &LayoutEngineRequest,
    registry: &LayoutEngineRegistry,
    context: &LayoutContext,
) -> Result<GraphTransaction, LayoutError> {
    layout_graph_to_transaction_with_engine(graph, request, registry, context)
}

/// Runs a selected layout engine and commits the resulting transaction through normal store dispatch.
pub fn apply_layout(
    store: &mut NodeGraphStore,
    request: &LayoutEngineRequest,
    registry: &LayoutEngineRegistry,
) -> Result<LayoutApplyOutcome, LayoutApplyError> {
    let context = layout_context_from_store(store);
    let layout = plan_layout(store.graph(), request, registry, &context)?;
    let tx = layout.to_transaction(store.graph())?;
    let dispatch = if tx.is_empty() {
        None
    } else {
        Some(store.dispatch_transaction(&tx)?)
    };

    Ok(LayoutApplyOutcome { layout, dispatch })
}

/// Runs dugong layout for a graph without mutating runtime state.
pub fn plan_dugong_layout(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<LayoutResult, LayoutError> {
    layout_graph_with_dugong(graph, request)
}

/// Runs dugong layout and returns the transaction that would move changed nodes.
pub fn dugong_layout_transaction(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<GraphTransaction, LayoutError> {
    plan_dugong_layout(graph, request)?.to_transaction(graph)
}

/// Runs dugong layout and commits the resulting transaction through normal store dispatch.
pub fn apply_dugong_layout(
    store: &mut NodeGraphStore,
    request: &LayoutRequest,
) -> Result<DugongLayoutApplyOutcome, DugongLayoutApplyError> {
    let registry = builtin_layout_engine_registry();
    let request = LayoutEngineRequest::dugong(request.clone());
    apply_layout(store, &request, &registry)
        .map(Into::into)
        .map_err(Into::into)
}

impl NodeGraphStore {
    /// Builds a layout context from non-persisted runtime facts already known by this store.
    pub fn layout_context(&self) -> LayoutContext {
        layout_context_from_store(self)
    }

    /// Runs a selected layout engine for the current graph without mutating the store.
    pub fn plan_layout(
        &self,
        request: &LayoutEngineRequest,
        registry: &LayoutEngineRegistry,
    ) -> Result<LayoutResult, LayoutError> {
        let context = self.layout_context();
        plan_layout(self.graph(), request, registry, &context)
    }

    /// Runs a selected layout engine and returns the transaction that would move changed nodes.
    pub fn layout_transaction(
        &self,
        request: &LayoutEngineRequest,
        registry: &LayoutEngineRegistry,
    ) -> Result<GraphTransaction, LayoutError> {
        let context = self.layout_context();
        layout_transaction(self.graph(), request, registry, &context)
    }

    /// Runs a selected layout engine and commits the resulting transaction through normal dispatch.
    pub fn apply_layout(
        &mut self,
        request: &LayoutEngineRequest,
        registry: &LayoutEngineRegistry,
    ) -> Result<LayoutApplyOutcome, LayoutApplyError> {
        apply_layout(self, request, registry)
    }

    /// Runs dugong layout for the current graph without mutating the store.
    pub fn plan_dugong_layout(&self, request: &LayoutRequest) -> Result<LayoutResult, LayoutError> {
        let registry = builtin_layout_engine_registry();
        let request = LayoutEngineRequest::dugong(request.clone());
        self.plan_layout(&request, &registry)
    }

    /// Runs dugong layout and returns the transaction that would move changed nodes.
    pub fn dugong_layout_transaction(
        &self,
        request: &LayoutRequest,
    ) -> Result<GraphTransaction, LayoutError> {
        let registry = builtin_layout_engine_registry();
        let request = LayoutEngineRequest::dugong(request.clone());
        self.layout_transaction(&request, &registry)
    }

    /// Runs dugong layout and commits the resulting transaction through normal store dispatch.
    pub fn apply_dugong_layout(
        &mut self,
        request: &LayoutRequest,
    ) -> Result<DugongLayoutApplyOutcome, DugongLayoutApplyError> {
        apply_dugong_layout(self, request)
    }
}
