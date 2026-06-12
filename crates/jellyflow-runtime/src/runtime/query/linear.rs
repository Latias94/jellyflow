use crate::runtime::binding::{BindingQueryOptions, BindingQueryResult};
use crate::runtime::measurement::LayoutFactsQueryResult;
use crate::runtime::rendering::RenderingQueryResult;
use jellyflow_core::core::CanvasSize;

use super::backend::{NodeGraphQuerySnapshot, QueryBackend, QueryBackendKind};
use super::bindings::resolve_binding_read_model;
use super::layout_facts::resolve_layout_facts_read_model;
use super::rendering::resolve_rendering_read_model;

/// Deterministic linear query backend.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct LinearQueryBackend;

impl QueryBackend for LinearQueryBackend {
    fn kind(&self) -> QueryBackendKind {
        QueryBackendKind::Linear
    }

    fn rendering_query(
        &self,
        snapshot: &NodeGraphQuerySnapshot<'_>,
        viewport_size: CanvasSize,
    ) -> RenderingQueryResult {
        resolve_rendering_read_model(snapshot, viewport_size)
    }

    fn layout_facts_query(
        &self,
        snapshot: &NodeGraphQuerySnapshot<'_>,
        viewport_size: CanvasSize,
    ) -> LayoutFactsQueryResult {
        resolve_layout_facts_read_model(snapshot, viewport_size)
    }

    fn binding_query(
        &self,
        snapshot: &NodeGraphQuerySnapshot<'_>,
        options: BindingQueryOptions,
    ) -> BindingQueryResult {
        resolve_binding_read_model(snapshot, options)
    }
}
