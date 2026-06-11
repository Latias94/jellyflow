//! Headless automatic layout integration.
//!
//! This module is a thin runtime facade over `jellyflow-layout`: it turns a layout request into a
//! normal graph transaction, then lets the store's dispatch/profile pipeline apply it.

use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};
use jellyflow_core::core::Graph;
use jellyflow_core::ops::GraphTransaction;
pub use jellyflow_layout::{
    LayoutDirection, LayoutEdgeRoute, LayoutError, LayoutNodePosition, LayoutOptions,
    LayoutRequest, LayoutResult, LayoutScope, LayoutSpacing, layout_graph_with_dugong,
};

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
    let layout = plan_dugong_layout(store.graph(), request)?;
    let tx = layout.to_transaction(store.graph())?;
    let dispatch = if tx.is_empty() {
        None
    } else {
        Some(store.dispatch_transaction(&tx)?)
    };

    Ok(DugongLayoutApplyOutcome { layout, dispatch })
}

impl NodeGraphStore {
    /// Runs dugong layout for the current graph without mutating the store.
    pub fn plan_dugong_layout(&self, request: &LayoutRequest) -> Result<LayoutResult, LayoutError> {
        plan_dugong_layout(self.graph(), request)
    }

    /// Runs dugong layout and returns the transaction that would move changed nodes.
    pub fn dugong_layout_transaction(
        &self,
        request: &LayoutRequest,
    ) -> Result<GraphTransaction, LayoutError> {
        dugong_layout_transaction(self.graph(), request)
    }

    /// Runs dugong layout and commits the resulting transaction through normal store dispatch.
    pub fn apply_dugong_layout(
        &mut self,
        request: &LayoutRequest,
    ) -> Result<DugongLayoutApplyOutcome, DugongLayoutApplyError> {
        apply_dugong_layout(self, request)
    }
}
